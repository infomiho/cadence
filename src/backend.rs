use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Sender as StdSender},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, anyhow};
use librespot::{core::SpotifyUri, playback::player::PlayerEvent};
use tokio::sync::mpsc::{Receiver, Sender, UnboundedReceiver, UnboundedSender};

use crate::{
    model::{Album, Artist, Playlist, Track, UserProfile},
    playback::{Playback, PlaybackAuthorization, delete_playback_refresh_token},
    spotify::{
        ClientIdSource, Spotify, SpotifyConfiguration, resolve_configuration, valid_client_id,
    },
    storage::{PlaybackSnapshot, Store},
};

const CATALOG_TIMEOUT_SECONDS: u64 = 30;
const COMMAND_CAPACITY: usize = 256;
const CONTROL_CAPACITY: usize = 8;

struct AuthorizationSuccess {
    playback: Option<PlaybackConnectionRequest>,
}

struct PlaybackConnectionRequest {
    load_saved_token: bool,
    authorization: Option<PlaybackAuthorization>,
}

#[derive(Clone)]
struct BlockingStore {
    jobs: std::sync::mpsc::Sender<StoreJob>,
}

type StoreJob = Box<dyn FnOnce(&mut Store) + Send>;
type RadioTask = tokio::task::JoinHandle<(u64, Result<Vec<Track>>)>;

impl BlockingStore {
    async fn open_default() -> Result<Self> {
        let (jobs, receiver) = std::sync::mpsc::channel::<StoreJob>();
        let (initialized, initialization) = tokio::sync::oneshot::channel();
        std::thread::Builder::new()
            .name("cadence-storage".to_owned())
            .spawn(move || {
                let mut store = match Store::open_default() {
                    Ok(store) => store,
                    Err(error) => {
                        let _ = initialized.send(Err(error.to_string()));
                        return;
                    }
                };
                let _ = initialized.send(Ok(()));
                while let Ok(job) = receiver.recv() {
                    job(&mut store);
                }
            })
            .context("could not start storage worker")?;
        initialization
            .await
            .context("storage worker stopped during initialization")?
            .map_err(anyhow::Error::msg)?;
        Ok(Self { jobs })
    }

    #[cfg(test)]
    fn from_store(mut store: Store) -> Self {
        let (jobs, receiver) = std::sync::mpsc::channel::<StoreJob>();
        std::thread::spawn(move || {
            while let Ok(job) = receiver.recv() {
                job(&mut store);
            }
        });
        Self { jobs }
    }

    async fn call<T>(
        &self,
        operation: impl FnOnce(&mut Store) -> Result<T> + Send + 'static,
    ) -> Result<T>
    where
        T: Send + 'static,
    {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.jobs
            .send(Box::new(move |store| {
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(store)))
                        .map_err(|_| anyhow!("storage operation panicked"))
                        .and_then(|result| result);
                let _ = sender.send(result);
            }))
            .map_err(|_| anyhow!("storage worker is unavailable"))?;
        receiver
            .await
            .context("storage operation did not complete")?
    }

    async fn playback_state(&self) -> Result<Option<PlaybackSnapshot>> {
        self.call(|store| store.playback_state()).await
    }

    async fn local_state(&self) -> Result<(Vec<Track>, Vec<Playlist>, Vec<Track>)> {
        self.call(|store| {
            Ok((
                store.favorites()?,
                store.pinned_playlists()?,
                store.recent_tracks(100)?,
            ))
        })
        .await
    }

    async fn liked_tracks(&self) -> Result<Vec<Track>> {
        self.call(|store| store.liked_tracks()).await
    }

    async fn favorites(&self) -> Result<Vec<Track>> {
        self.call(|store| store.favorites()).await
    }

    async fn remove_spotify_client_id(&self) -> Result<()> {
        self.call(|store| store.remove_spotify_client_id()).await
    }

    async fn configure_spotify(&self, client_id: String) -> Result<()> {
        self.call(move |store| store.configure_spotify(&client_id))
            .await
    }

    async fn reset_spotify_configuration(&self) -> Result<()> {
        self.call(|store| store.reset_spotify_configuration()).await
    }

    async fn set_oauth_credentials_invalidated(&self, invalidated: bool) -> Result<()> {
        self.call(move |store| store.set_spotify_oauth_credentials_invalidated(invalidated))
            .await
    }

    async fn set_playback_credentials_invalidated(&self, invalidated: bool) -> Result<()> {
        self.call(move |store| store.set_spotify_playback_credentials_invalidated(invalidated))
            .await
    }

    async fn replace_liked_tracks(&self, tracks: Vec<Track>) -> Result<()> {
        self.call(move |store| store.replace_liked_tracks(&tracks))
            .await
    }

    async fn replace_liked_tracks_if_current(
        &self,
        tracks: Vec<Track>,
        current_generation: Arc<AtomicU64>,
        generation: u64,
    ) -> Result<Option<Vec<Track>>> {
        self.call(move |store| {
            if current_generation.load(Ordering::Acquire) != generation {
                return Ok(None);
            }
            store.replace_liked_tracks(&tracks)?;
            Ok(Some(tracks))
        })
        .await
    }

    async fn clear_playback_state(&self) -> Result<()> {
        self.call(|store| store.clear_playback_state()).await
    }

    async fn set_playback_state(
        &self,
        tracks: Vec<Track>,
        index: usize,
        position_ms: u32,
    ) -> Result<()> {
        self.call(move |store| store.set_playback_state(&tracks, index, position_ms))
            .await
    }

    async fn update_playback_position(&self, position_ms: u32) -> Result<()> {
        self.call(move |store| store.update_playback_position(position_ms))
            .await
    }

    async fn set_favorite(&self, track: Track, favorite: bool) -> Result<()> {
        self.call(move |store| store.set_favorite(&track, favorite))
            .await
    }

    async fn set_playlist_pinned(&self, playlist: Playlist, pinned: bool) -> Result<()> {
        self.call(move |store| store.set_playlist_pinned(&playlist, pinned))
            .await
    }

    async fn add_history(&self, track: Track) -> Result<()> {
        self.call(move |store| store.add_history(&track)).await
    }
}

/// Where a catalog request sends its answer. Dropping the receiving half
/// cancels the request: the reply simply goes nowhere.
pub type Reply<T> = tokio::sync::oneshot::Sender<Result<T>>;

pub type LibraryContents = (Vec<Track>, Vec<Playlist>);
pub type SearchResults = (Vec<Track>, Vec<Playlist>);
pub type ArtistDetails = (Artist, Vec<Track>, Vec<Album>);
pub type AlbumDetails = (Album, Vec<Track>);

#[derive(Debug)]
pub enum BackendCommand {
    Authenticate {
        generation: u64,
    },
    Logout {
        generation: u64,
    },
    ConfigureSpotify {
        generation: u64,
        client_id: String,
    },
    ResetSpotifyConfiguration {
        generation: u64,
    },
    ReloadLibrary {
        respond: Reply<LibraryContents>,
    },
    SearchCatalog {
        query: String,
        respond: Reply<SearchResults>,
    },
    LoadPlaylist {
        playlist: Playlist,
        respond: Reply<Vec<Track>>,
    },
    LoadArtist {
        source_id: String,
        respond: Reply<ArtistDetails>,
    },
    LoadAlbum {
        source_id: String,
        respond: Reply<AlbumDetails>,
    },
    StartRadio {
        request_id: u64,
        seed: Track,
    },
    PlayContext {
        tracks: Vec<Track>,
        index: usize,
    },
    PlayNext(Track),
    AppendToQueue(Track),
    RestorePlayback {
        position_ms: u32,
        playing: bool,
    },
    SetFavorite {
        track: Track,
        favorite: bool,
    },
    SetPlaylistPinned {
        playlist: Playlist,
        pinned: bool,
    },
    Resume,
    Pause,
    Next,
    Previous,
    Seek(u32),
    SavePlaybackPosition {
        spotify_uri: String,
        position_ms: u32,
    },
    SetVolume(f32),
    Shutdown {
        acknowledged: StdSender<()>,
    },
}

#[derive(Debug)]
pub enum BackendEvent {
    SetupRequired,
    SpotifyConfigured {
        generation: u64,
        client_id: String,
        source: ClientIdSource,
    },
    SpotifyConfigurationFailed {
        generation: u64,
        error: String,
    },
    SpotifyConfigurationResetFailed(String),
    AuthorizationRequired,
    LoggedOut,
    CatalogReady {
        generation: u64,
    },
    PlaybackReady,
    PlaybackReconnecting,
    PlaybackReconnected,
    PlaybackRestored {
        position_ms: u32,
        playing: bool,
    },
    PlaybackSettled,
    QueueEnded,
    LibraryLoaded {
        generation: u64,
        liked_tracks: Vec<Track>,
        playlists: Vec<Playlist>,
    },
    ProfileLoaded {
        generation: u64,
        profile: UserProfile,
    },
    CachedLikedTracks {
        generation: u64,
        tracks: Vec<Track>,
    },
    LocalStateLoaded {
        favorites: Vec<Track>,
        pinned_playlists: Vec<Playlist>,
        recently_played: Vec<Track>,
    },
    Playing {
        spotify_uri: String,
    },
    Loading {
        spotify_uri: String,
    },
    Paused {
        spotify_uri: String,
    },
    EndOfTrack {
        spotify_uri: String,
    },
    PositionChanged {
        spotify_uri: String,
        position_ms: u32,
    },
    PlaybackContext {
        current: Track,
        next: Vec<Track>,
    },
    PlaybackSnapshotLoaded {
        current: Track,
        next: Vec<Track>,
        position_ms: u32,
    },
    AuthorizationFailed(String),
    CatalogFailed {
        generation: u64,
        error: String,
    },
    PlaybackFailed(String),
    TrackFailed {
        spotify_uri: String,
        error: String,
    },
    RadioFailed {
        request_id: u64,
        error: String,
    },
    RadioStarted {
        request_id: u64,
    },
    RadioCancelled {
        request_id: u64,
    },
    FatalError(String),
    Error(String),
}

/// The sending half of the backend. Cloning one is cheap, so views can hold a
/// handle without owning the worker thread that plays music.
#[derive(Clone)]
pub struct BackendHandle {
    commands: Sender<BackendCommand>,
    controls: Sender<BackendCommand>,
    volume: tokio::sync::watch::Sender<f32>,
}

impl BackendHandle {
    pub fn send(&self, command: BackendCommand) -> bool {
        send_command(&self.commands, &self.controls, &self.volume, command)
    }
}

/// Owns the backend worker thread. Dropping this stops playback, so it is held
/// by a process-wide service rather than by a window.
pub struct Backend {
    handle: BackendHandle,
    shutdown: tokio::sync::watch::Sender<bool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Backend {
    pub fn start() -> (Self, UnboundedReceiver<BackendEvent>) {
        let (commands, command_receiver) = tokio::sync::mpsc::channel(COMMAND_CAPACITY);
        let (controls, control_receiver) = tokio::sync::mpsc::channel(CONTROL_CAPACITY);
        let (event_sender, events) = tokio::sync::mpsc::unbounded_channel();
        let (volume, volume_receiver) = tokio::sync::watch::channel(0.72);
        let (shutdown, shutdown_receiver) = tokio::sync::watch::channel(false);
        let thread = thread::Builder::new()
            .name("cadence-backend".to_owned())
            .spawn(move || {
                let runtime = tokio::runtime::Runtime::new().expect("could not start Tokio");
                let acknowledged = runtime.block_on(run(
                    command_receiver,
                    control_receiver,
                    event_sender,
                    volume_receiver,
                    shutdown_receiver,
                ));
                runtime.shutdown_timeout(Duration::from_secs(1));
                if let Some(acknowledged) = acknowledged {
                    let _ = acknowledged.send(());
                }
            })
            .expect("could not start the Cadence backend");
        (
            Self {
                handle: BackendHandle {
                    commands,
                    controls,
                    volume,
                },
                shutdown,
                thread: Some(thread),
            },
            events,
        )
    }

    pub fn handle(&self) -> BackendHandle {
        self.handle.clone()
    }
}

fn send_command(
    commands: &Sender<BackendCommand>,
    controls: &Sender<BackendCommand>,
    volume_sender: &tokio::sync::watch::Sender<f32>,
    command: BackendCommand,
) -> bool {
    if matches!(
        &command,
        BackendCommand::Authenticate { .. } | BackendCommand::Logout { .. }
    ) {
        return controls.try_send(command).is_ok();
    }
    match command {
        BackendCommand::SetVolume(volume) => volume_sender.send(volume).is_ok(),
        command => commands.try_send(command).is_ok(),
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        let (acknowledged, acknowledgment) = mpsc::channel();
        let _ = self.shutdown.send(true);
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut command = BackendCommand::Shutdown { acknowledged };
        let sent = loop {
            match self.handle.commands.try_send(command) {
                Ok(()) => break true,
                Err(tokio::sync::mpsc::error::TrySendError::Full(returned))
                    if Instant::now() < deadline =>
                {
                    command = returned;
                    thread::sleep(Duration::from_millis(1));
                }
                Err(_) => break false,
            }
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        let stopped = sent && acknowledgment.recv_timeout(remaining).is_ok();
        if stopped && let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Everything `run` needs once Spotify is usable.
struct Startup {
    store: BlockingStore,
    spotify: Spotify,
    configuration: Option<SpotifyConfiguration>,
    playback_snapshot: Option<PlaybackSnapshot>,
    playback_credentials_invalidated: bool,
}

/// Opens storage and resolves a usable Spotify configuration, walking the
/// listener through setup when none is saved yet.
///
/// The error carries the shutdown acknowledgment when the app quit before
/// setup finished, so the caller can hand it back to whoever asked to stop.
async fn start(
    commands: &mut Receiver<BackendCommand>,
    events: &UnboundedSender<BackendEvent>,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<Startup, Option<StdSender<()>>> {
    let store = match BlockingStore::open_default().await {
        Ok(store) => store,
        Err(error) => {
            send_fatal_error(events, error);
            return Err(None);
        }
    };
    send_local_state(&store, events).await;
    let playback_snapshot = match store.playback_state().await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            send_error(events, error);
            None
        }
    };
    if let Some(snapshot) = &playback_snapshot
        && let Some(current) = snapshot.tracks.get(snapshot.index)
    {
        let _ = events.send(BackendEvent::PlaybackSnapshotLoaded {
            current: current.clone(),
            next: snapshot
                .tracks
                .get(snapshot.index + 1..)
                .unwrap_or_default()
                .to_vec(),
            position_ms: snapshot.position_ms,
        });
    }
    let environment_client_id = std::env::var("SPOTIFY_CLIENT_ID").ok();
    let saved_client_id = match store.call(|store| store.spotify_client_id()).await {
        Ok(client_id) => client_id,
        Err(error) => {
            send_fatal_error(events, error);
            return Err(None);
        }
    };
    let oauth_credentials_invalidated = match store
        .call(|store| store.spotify_oauth_credentials_invalidated())
        .await
    {
        Ok(invalidated) => invalidated,
        Err(error) => {
            send_fatal_error(events, error);
            return Err(None);
        }
    };
    let mut playback_credentials_invalidated = match store
        .call(|store| store.spotify_playback_credentials_invalidated())
        .await
    {
        Ok(invalidated) => invalidated,
        Err(error) => {
            send_fatal_error(events, error);
            return Err(None);
        }
    };
    let mut configuration =
        resolve_configuration(environment_client_id.as_deref(), saved_client_id.as_deref());
    let mut configuration_generation = 0;
    if let Some(configured) = configuration.clone()
        && !valid_client_id(&configured.client_id)
    {
        if configured.source == ClientIdSource::Environment {
            let _ = events.send(BackendEvent::SpotifyConfigured {
                generation: 0,
                client_id: configured.client_id,
                source: ClientIdSource::Environment,
            });
            let _ = events.send(BackendEvent::SpotifyConfigurationFailed {
                generation: 0,
                error: "SPOTIFY_CLIENT_ID must contain 32 hexadecimal characters".to_owned(),
            });
            while let Some(command) = commands.recv().await {
                if let BackendCommand::Shutdown { acknowledged } = command {
                    return Err(Some(acknowledged));
                }
            }
            return Err(None);
        }
        if let Err(error) = store.call(|store| store.remove_spotify_client_id()).await {
            send_fatal_error(events, error);
            return Err(None);
        }
        configuration = None;
    }
    let spotify = loop {
        if let Some(configured) = &configuration {
            let spotify = tokio::select! {
                result = Spotify::from_client_id(
                    &configured.client_id,
                    !oauth_credentials_invalidated,
                ) => result,
                _ = wait_for_shutdown(shutdown) => return Err(receive_shutdown_acknowledgment(commands).await),
            };
            match spotify {
                Ok(spotify) => break spotify,
                Err(error) => {
                    let _ = events.send(BackendEvent::SpotifyConfigurationFailed {
                        generation: configuration_generation,
                        error: error.to_string(),
                    });
                    if configured.source == ClientIdSource::Saved {
                        let _ = store.remove_spotify_client_id().await;
                        configuration = None;
                        continue;
                    }
                    return Err(None);
                }
            }
        }

        let _ = events.send(BackendEvent::SetupRequired);
        let command = tokio::select! {
            command = commands.recv() => command.ok_or(None)?,
            _ = wait_for_shutdown(shutdown) => return Err(receive_shutdown_acknowledgment(commands).await),
        };
        match command {
            BackendCommand::ConfigureSpotify {
                generation,
                client_id,
            } if valid_client_id(&client_id) => {
                let client_id = client_id.trim().to_owned();
                let candidate = tokio::select! {
                    result = Spotify::from_client_id(&client_id, false) => result,
                    _ = wait_for_shutdown(shutdown) => return Err(receive_shutdown_acknowledgment(commands).await),
                };
                let candidate = match candidate {
                    Ok(candidate) => candidate,
                    Err(error) => {
                        let _ = events.send(BackendEvent::SpotifyConfigurationFailed {
                            generation,
                            error: error.to_string(),
                        });
                        continue;
                    }
                };
                if let Err(error) = store.configure_spotify(client_id.clone()).await {
                    let _ = events.send(BackendEvent::SpotifyConfigurationFailed {
                        generation,
                        error: error.to_string(),
                    });
                    continue;
                }
                configuration = Some(SpotifyConfiguration {
                    client_id,
                    source: ClientIdSource::Saved,
                });
                configuration_generation = generation;
                playback_credentials_invalidated = true;
                break candidate;
            }
            BackendCommand::ConfigureSpotify { generation, .. } => {
                let _ = events.send(BackendEvent::SpotifyConfigurationFailed {
                    generation,
                    error: "Spotify Client ID must contain 32 hexadecimal characters".to_owned(),
                });
            }
            BackendCommand::Shutdown { acknowledged } => return Err(Some(acknowledged)),
            _ => {}
        }
    };
    let configured = configuration
        .as_ref()
        .expect("Spotify configuration must exist after setup");
    let _ = events.send(BackendEvent::SpotifyConfigured {
        generation: configuration_generation,
        client_id: configured.client_id.clone(),
        source: configured.source,
    });
    Ok(Startup {
        store,
        spotify,
        configuration,
        playback_snapshot,
        playback_credentials_invalidated,
    })
}

async fn run(
    mut commands: Receiver<BackendCommand>,
    mut controls: Receiver<BackendCommand>,
    events: UnboundedSender<BackendEvent>,
    mut volume: tokio::sync::watch::Receiver<f32>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Option<StdSender<()>> {
    let Startup {
        store,
        mut spotify,
        mut configuration,
        playback_snapshot,
        mut playback_credentials_invalidated,
    } = match start(&mut commands, &events, &mut shutdown).await {
        Ok(startup) => startup,
        Err(acknowledged) => return acknowledged,
    };
    let mut playback_tracks = playback_snapshot
        .as_ref()
        .map(|snapshot| snapshot.tracks.clone())
        .unwrap_or_default();
    let mut playback_index = playback_snapshot.as_ref().map(|snapshot| snapshot.index);
    let mut playback_position_ms = playback_snapshot
        .as_ref()
        .map_or(0, |snapshot| snapshot.position_ms);
    let mut account_generation = 0;
    let catalog_generation = Arc::new(AtomicU64::new(account_generation));
    let mut connection = PlaybackConnection::default();
    let mut favorite_refresh_task = None;
    let mut catalog_tasks = Vec::new();
    let mut catalog = CatalogFetches::default();
    let mut library_reload_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut radio_task: Option<RadioTask> = None;
    let mut radio_request_id = None;
    let mut authorization_task: Option<
        tokio::task::JoinHandle<(u64, Result<AuthorizationSuccess>)>,
    > = None;
    let mut logout_task: Option<tokio::task::JoinHandle<Result<()>>> = None;
    let is_authorized = tokio::select! {
        authorized = spotify.is_authorized() => match authorized {
            Ok(authorized) => authorized,
            Err(error) => {
                send_fatal_error(&events, error);
                return None;
            }
        },
        _ = wait_for_shutdown(&mut shutdown) => return receive_shutdown_acknowledgment(&mut commands).await,
    };
    if is_authorized {
        match store.liked_tracks().await {
            Ok(tracks) if !tracks.is_empty() => {
                let _ = events.send(BackendEvent::CachedLikedTracks {
                    generation: account_generation,
                    tracks,
                });
            }
            Ok(_) => {}
            Err(error) => send_error(&events, error),
        }
        catalog_tasks.push(spawn_library_load(
            spotify.clone(),
            store.clone(),
            account_generation,
            catalog_generation.clone(),
            events.clone(),
        ));
        favorite_refresh_task = spawn_favorite_refresh(&store, spotify.clone(), &events).await;
        let connected = tokio::select! {
            result = connect_playback(&events, true, None) => result,
            _ = wait_for_shutdown(&mut shutdown) => {
                for task in catalog_tasks.drain(..) {
                    task.abort();
                }
                catalog.abort_all();
                cancel_radio(&mut radio_task, &mut radio_request_id, &events);
                abort_task(&mut logout_task);
                if let Some(task) = favorite_refresh_task.take() {
                    task.abort();
                }
                return receive_shutdown_acknowledgment(&mut commands).await;
            },
        };
        match connected {
            Ok((player, observer)) => {
                connection.player = Some(player);
                connection.observer = Some(observer);
                if let Err(error) = restore_saved_playback(
                    &connection.player,
                    &playback_tracks,
                    playback_index,
                    playback_position_ms,
                    &events,
                ) {
                    send_error(&events, error);
                }
            }
            Err(error) => {
                let _ = events.send(BackendEvent::PlaybackFailed(error.to_string()));
            }
        }
    } else {
        let _ = events.send(BackendEvent::AuthorizationRequired);
    }

    let mut playback_health = tokio::time::interval(std::time::Duration::from_secs(5));
    playback_health.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let command = tokio::select! {
            command = controls.recv() => {
                command?
            }
            command = commands.recv() => {
                command?
            }
            _ = wait_for_shutdown(&mut shutdown) => {
                if let Some(task) = logout_task.take() {
                    let _ = task.await;
                }
                return receive_shutdown_acknowledgment(&mut commands).await;
            }
            changed = volume.changed() => {
                if changed.is_ok() && let Some(player) = &connection.player {
                    player.set_volume(*volume.borrow_and_update());
                }
                continue;
            }
            _ = playback_health.tick() => {
                if connection.reconnect_pending
                    || connection.player.as_ref().is_some_and(|player| !player.is_connected())
                {
                    connection.reconnect_pending = true;
                    if connection.reconnect.is_none() {
                        log::info!("playback: connection lost, reconnecting");
                        connection.disconnect();
                        let _ = events.send(BackendEvent::PlaybackReconnecting);
                        connection.reconnect = Some(tokio::spawn(async {
                            tokio::time::timeout(
                                std::time::Duration::from_secs(15),
                                Playback::reconnect(),
                            )
                            .await
                            .context("Spotify playback reconnection timed out")?
                        }));
                    }
                }
                continue;
            }
            attempt = connection.settled() => {
                match attempt {
                    Attempt::Reconnected(reconnected) => {
                        connection.reconnect = None;
                        match reconnected {
                            Ok(Ok(player)) => {
                                log::info!("playback: reconnected");
                                connection.adopt(player, &events);
                                connection.reconnect_pending = false;
                                let _ = events.send(BackendEvent::PlaybackReconnected);
                            }
                            Ok(Err(error)) => {
                                log::warn!("playback: reconnect attempt failed: {error}");
                                let _ = events.send(BackendEvent::PlaybackFailed(format!(
                                    "Spotify playback disconnected; reconnecting: {error}"
                                )));
                            }
                            Err(error) => send_error(&events, error),
                        }
                    }
                    Attempt::Connected(connected) => {
                        connection.connect = None;
                        match connected {
                            Ok(Ok(player)) => {
                                log::info!("playback: connected");
                                connection.adopt(player, &events);
                                playback_credentials_invalidated = false;
                                if let Err(error) =
                                    store.set_playback_credentials_invalidated(false).await
                                {
                                    send_error(&events, error);
                                }
                                let _ = events.send(BackendEvent::PlaybackReady);
                                if connection.connect_restoring {
                                    let _ = events.send(BackendEvent::PlaybackReconnected);
                                } else if let Err(error) = restore_saved_playback(
                                    &connection.player,
                                    &playback_tracks,
                                    playback_index,
                                    playback_position_ms,
                                    &events,
                                ) {
                                    send_error(&events, error);
                                }
                            }
                            Ok(Err(error)) => {
                                let _ = events.send(BackendEvent::PlaybackFailed(error.to_string()));
                            }
                            Err(error) => send_error(&events, error),
                        }
                        connection.connect_restoring = false;
                    }
                }
                continue;
            }
            refreshed = finished(&mut favorite_refresh_task) => {
                favorite_refresh_task = None;
                match refreshed {
                    Some(Ok(Ok(tracks))) => {
                        let mut changed = false;
                        for track in tracks {
                            match store.set_favorite(track, true).await {
                                Ok(()) => changed = true,
                                Err(error) => send_error(&events, error),
                            }
                        }
                        if changed {
                            send_local_state(&store, &events).await;
                        }
                    }
                    Some(Ok(Err(error))) => send_error(&events, error),
                    Some(Err(error)) => send_error(&events, error),
                    None => {}
                }
                continue;
            }
            radio = finished(&mut radio_task) => {
                radio_task = None;
                radio_request_id = None;
                match radio {
                    Some(Ok((request_id, Ok(tracks)))) => {
                        match load_context_track(&connection.player, &tracks, 0, &store, &events).await {
                            Ok(()) => {
                                playback_tracks = tracks;
                                playback_index = Some(0);
                                playback_position_ms = 0;
                                if let Err(error) =
                                    store.set_playback_state(playback_tracks.clone(), 0, 0).await
                                {
                                    send_error(&events, error);
                                }
                                let _ = events.send(BackendEvent::RadioStarted { request_id });
                            }
                            Err(error) => {
                                let _ = events.send(BackendEvent::RadioFailed {
                                    request_id,
                                    error: error.to_string(),
                                });
                                let _ = events.send(BackendEvent::PlaybackSettled);
                            }
                        }
                    }
                    Some(Ok((request_id, Err(error)))) => {
                        let _ = events.send(BackendEvent::RadioFailed {
                            request_id,
                            error: error.to_string(),
                        });
                        let _ = events.send(BackendEvent::PlaybackSettled);
                    }
                    Some(Err(error)) => send_error(&events, error),
                    None => {}
                }
                continue;
            }
            authorization = finished(&mut authorization_task) => {
                authorization_task = None;
                match authorization {
                    Some(Ok((generation, Ok(success)))) => {
                        if let Err(error) = store.set_oauth_credentials_invalidated(false).await {
                            send_error(&events, error);
                        }
                        account_generation = generation;
                        catalog_generation.store(generation, Ordering::Release);
                        for task in catalog_tasks.drain(..) {
                            task.abort();
                        }
                        catalog.abort_all();
                        cancel_radio(&mut radio_task, &mut radio_request_id, &events);
                        if let Some(task) = favorite_refresh_task.take() {
                            task.abort();
                        }
                        catalog_tasks.push(spawn_library_load(
                            spotify.clone(),
                            store.clone(),
                            account_generation,
                            catalog_generation.clone(),
                            events.clone(),
                        ));
                        favorite_refresh_task =
                            spawn_favorite_refresh(&store, spotify.clone(), &events).await;

                        if let Some(request) = success.playback {
                            connection.connect_restoring = connection.reconnect_pending;
                            connection.abort_attempts();
                            connection.reconnect_pending = false;
                            connection.disconnect();
                            connection.connect = Some(tokio::spawn(Playback::connect(
                                request.load_saved_token,
                                request.authorization,
                            )));
                        }
                    }
                    Some(Ok((_, Err(error)))) => {
                        let _ = events.send(BackendEvent::AuthorizationFailed(error.to_string()));
                    }
                    Some(Err(error)) => send_error(&events, error),
                    None => {}
                }
                continue;
            }
            logout = finished(&mut logout_task) => {
                logout_task = None;
                match logout {
                    Some(Ok(Ok(()))) => {
                        let _ = events.send(BackendEvent::LoggedOut);
                    }
                    Some(Ok(Err(error))) => {
                        let _ = events.send(BackendEvent::LoggedOut);
                        send_error(&events, error);
                    }
                    Some(Err(error)) => send_error(&events, error),
                    None => {}
                }
                continue;
            }
        };
        catalog_tasks.retain(|task| !task.is_finished());
        if logout_task.is_some()
            && !matches!(
                &command,
                BackendCommand::Logout { .. } | BackendCommand::Shutdown { .. }
            )
        {
            send_error(&events, "Spotify logout is still finishing");
            continue;
        }
        let result = match command {
            BackendCommand::ResetSpotifyConfiguration { generation } => {
                abort_task(&mut authorization_task);
                abort_task(&mut connection.connect);
                catalog_generation.store(generation, Ordering::Release);
                if configuration.as_ref().is_some_and(|configuration| {
                    configuration.source == ClientIdSource::Environment
                }) {
                    let _ = events.send(BackendEvent::SpotifyConfigurationResetFailed(
                        "SPOTIFY_CLIENT_ID is configured by the environment and cannot be changed in Cadence".to_owned(),
                    ));
                    Ok(())
                } else if let Err(error) = store.reset_spotify_configuration().await {
                    let _ = events.send(BackendEvent::SpotifyConfigurationResetFailed(
                        error.to_string(),
                    ));
                    Ok(())
                } else {
                    for task in catalog_tasks.drain(..) {
                        task.abort();
                    }
                    catalog.abort_all();
                    cancel_radio(&mut radio_task, &mut radio_request_id, &events);
                    if let Some(task) = favorite_refresh_task.take() {
                        task.abort();
                    }
                    connection.disconnect();
                    abort_task(&mut connection.reconnect);
                    playback_tracks.clear();
                    playback_index = None;
                    connection.reconnect_pending = false;
                    playback_credentials_invalidated = true;
                    configuration = None;

                    if let Err(error) = spotify.logout().await {
                        send_error(&events, error);
                    }
                    if let Err(error) = delete_playback_refresh_token().await {
                        send_error(&events, error);
                    }
                    if let Err(error) = store.replace_liked_tracks(Vec::new()).await {
                        send_error(&events, error);
                    }
                    if let Err(error) = store.clear_playback_state().await {
                        send_error(&events, error);
                    }

                    let _ = events.send(BackendEvent::LoggedOut);
                    let _ = events.send(BackendEvent::SetupRequired);
                    Ok(())
                }
            }
            BackendCommand::ConfigureSpotify {
                generation,
                client_id,
            } => {
                abort_task(&mut authorization_task);
                abort_task(&mut connection.connect);
                if configuration.as_ref().is_some_and(|configuration| {
                    configuration.source == ClientIdSource::Environment
                }) {
                    let _ = events.send(BackendEvent::SpotifyConfigurationFailed {
                        generation,
                        error: "SPOTIFY_CLIENT_ID is configured by the environment and cannot be changed in Cadence".to_owned(),
                    });
                    Ok(())
                } else if configuration.is_some() {
                    let _ = events.send(BackendEvent::SpotifyConfigurationFailed {
                        generation,
                        error: "Remove the current Spotify configuration before replacing it."
                            .to_owned(),
                    });
                    Ok(())
                } else if !valid_client_id(&client_id) {
                    let _ = events.send(BackendEvent::SpotifyConfigurationFailed {
                        generation,
                        error: "Spotify Client ID must contain 32 hexadecimal characters"
                            .to_owned(),
                    });
                    Ok(())
                } else {
                    let client_id = client_id.trim().to_owned();
                    let candidate = tokio::select! {
                        result = Spotify::from_client_id(&client_id, false) => result,
                        _ = wait_for_shutdown(&mut shutdown) => continue,
                    };
                    match candidate {
                        Err(error) => {
                            let _ = events.send(BackendEvent::SpotifyConfigurationFailed {
                                generation,
                                error: error.to_string(),
                            });
                            Ok(())
                        }
                        Ok(candidate) => match store.configure_spotify(client_id.clone()).await {
                            Err(error) => {
                                let _ = events.send(BackendEvent::SpotifyConfigurationFailed {
                                    generation,
                                    error: error.to_string(),
                                });
                                Ok(())
                            }
                            Ok(()) => {
                                spotify = candidate;
                                playback_credentials_invalidated = true;
                                configuration = Some(SpotifyConfiguration {
                                    client_id: client_id.clone(),
                                    source: ClientIdSource::Saved,
                                });
                                let _ = events.send(BackendEvent::SpotifyConfigured {
                                    generation,
                                    client_id,
                                    source: ClientIdSource::Saved,
                                });
                                let _ = events.send(BackendEvent::AuthorizationRequired);
                                Ok(())
                            }
                        },
                    }
                }
            }
            BackendCommand::Authenticate { generation } => {
                abort_task(&mut authorization_task);
                abort_task(&mut connection.connect);
                let needs_playback = connection
                    .player
                    .as_ref()
                    .is_none_or(|player| !player.is_connected());
                let spotify = spotify.clone();
                let playback_credentials_invalidated = playback_credentials_invalidated;
                authorization_task = Some(tokio::spawn(async move {
                    let result = authorize_account(
                        spotify,
                        needs_playback,
                        playback_credentials_invalidated,
                    )
                    .await;
                    (generation, result)
                }));
                Ok(())
            }
            BackendCommand::Logout { generation } => {
                abort_task(&mut logout_task);
                abort_task(&mut authorization_task);
                abort_task(&mut connection.connect);
                catalog_generation.store(generation, Ordering::Release);
                for task in catalog_tasks.drain(..) {
                    task.abort();
                }
                catalog.abort_all();
                cancel_radio(&mut radio_task, &mut radio_request_id, &events);
                if let Some(task) = favorite_refresh_task.take() {
                    task.abort();
                }
                connection.disconnect();
                playback_tracks.clear();
                playback_index = None;
                connection.reconnect_pending = false;
                if let Some(task) = connection.reconnect.take() {
                    task.abort();
                }
                playback_credentials_invalidated = true;
                logout_task = Some(tokio::spawn(logout_account(store.clone(), spotify.clone())));
                Ok(())
            }
            BackendCommand::ReloadLibrary { respond } => {
                let spotify = spotify.clone();
                let store = store.clone();
                let generation = account_generation;
                let current_generation = catalog_generation.clone();
                library_reload_task = Some(tokio::spawn(async move {
                    let loaded = run_with_timeout(60, "Spotify library request", async {
                        load_library(&spotify).await
                    })
                    .await;
                    // Keep the on-disk copy in step so the next launch paints
                    // the refreshed list before the network answers.
                    let loaded = match loaded {
                        Ok((liked_tracks, playlists)) => {
                            match persist_library_cache(
                                &store,
                                liked_tracks,
                                playlists,
                                current_generation,
                                generation,
                            )
                            .await
                            {
                                Ok(Some(contents)) => Ok(contents),
                                Ok(None) => Err(anyhow!("Spotify account changed while loading")),
                                Err(error) => Err(error),
                            }
                        }
                        Err(error) => Err(error),
                    };
                    let _ = respond.send(loaded);
                }));
                Ok(())
            }
            BackendCommand::SearchCatalog { query, respond } => {
                catalog.search(spotify.clone(), query, respond);
                Ok(())
            }
            BackendCommand::LoadPlaylist { playlist, respond } => {
                catalog.playlist(spotify.clone(), playlist, respond);
                Ok(())
            }
            BackendCommand::LoadArtist { source_id, respond } => {
                catalog.artist(spotify.clone(), source_id, respond);
                Ok(())
            }
            BackendCommand::LoadAlbum { source_id, respond } => {
                catalog.album(spotify.clone(), source_id, respond);
                Ok(())
            }
            BackendCommand::StartRadio { request_id, seed } => {
                cancel_radio(&mut radio_task, &mut radio_request_id, &events);
                radio_request_id = Some(request_id);
                let player = connection.player.clone();
                let spotify = spotify.clone();
                radio_task = Some(tokio::spawn(async move {
                    let result = tokio::time::timeout(Duration::from_secs(20), async {
                        let player = player.context("Spotify playback is not connected")?;
                        let seed_uri = seed
                            .spotify_uri
                            .as_deref()
                            .context("radio seed has no Spotify track URI")?;
                        let uris = player.radio_track_uris(seed_uri).await?;
                        let recommendations = spotify.resolve_track_uris(&uris).await?;
                        build_radio_context(seed, recommendations)
                    })
                    .await
                    .unwrap_or_else(|_| Err(anyhow!("Spotify track radio timed out")));
                    (request_id, result)
                }));
                Ok(())
            }
            BackendCommand::PlayContext { tracks, index } => {
                cancel_radio(&mut radio_task, &mut radio_request_id, &events);
                let spotify_uri = tracks
                    .get(index)
                    .and_then(|track| track.spotify_uri.clone())
                    .unwrap_or_default();
                match load_context_track(&connection.player, &tracks, index, &store, &events).await
                {
                    Ok(()) => {
                        playback_tracks = tracks;
                        playback_index = Some(index);
                        playback_position_ms = 0;
                        if let Err(error) = store
                            .set_playback_state(playback_tracks.clone(), index, 0)
                            .await
                        {
                            send_error(&events, error);
                        }
                    }
                    Err(error) => {
                        let _ = events.send(BackendEvent::TrackFailed {
                            spotify_uri,
                            error: error.to_string(),
                        });
                        let _ = events.send(BackendEvent::PlaybackSettled);
                    }
                }
                Ok(())
            }
            BackendCommand::PlayNext(track) => {
                if let Some(index) = playback_index {
                    let mut updated_tracks = playback_tracks.clone();
                    updated_tracks.insert(index + 1, track);
                    match store
                        .set_playback_state(updated_tracks.clone(), index, playback_position_ms)
                        .await
                    {
                        Ok(()) => {
                            playback_tracks = updated_tracks;
                            send_playback_context(&playback_tracks, index, &events);
                            Ok(())
                        }
                        Err(error) => Err(error),
                    }
                } else {
                    Err(anyhow!("Nothing is currently playing"))
                }
            }
            BackendCommand::AppendToQueue(track) => {
                if let Some(index) = playback_index {
                    let mut updated_tracks = playback_tracks.clone();
                    updated_tracks.push(track);
                    match store
                        .set_playback_state(updated_tracks.clone(), index, playback_position_ms)
                        .await
                    {
                        Ok(()) => {
                            playback_tracks = updated_tracks;
                            send_playback_context(&playback_tracks, index, &events);
                            Ok(())
                        }
                        Err(error) => Err(error),
                    }
                } else {
                    Err(anyhow!("Nothing is currently playing"))
                }
            }
            BackendCommand::RestorePlayback {
                position_ms,
                playing,
            } => {
                let result = restore_context_track(
                    &connection.player,
                    &playback_tracks,
                    playback_index,
                    position_ms,
                    playing,
                    &events,
                );
                if result.is_err() {
                    let _ = events.send(BackendEvent::PlaybackSettled);
                } else {
                    let _ = events.send(BackendEvent::PlaybackRestored {
                        position_ms,
                        playing,
                    });
                    playback_position_ms = position_ms;
                    if let Err(error) = store.update_playback_position(position_ms).await {
                        send_error(&events, error);
                    }
                }
                result
            }
            BackendCommand::SetFavorite { track, favorite } => {
                let result = store.set_favorite(track, favorite).await;
                if result.is_ok() {
                    send_local_state(&store, &events).await;
                }
                result
            }
            BackendCommand::SetPlaylistPinned { playlist, pinned } => {
                let result = store.set_playlist_pinned(playlist, pinned).await;
                if result.is_ok() {
                    send_local_state(&store, &events).await;
                }
                result
            }
            BackendCommand::Resume => {
                let result = connection
                    .player
                    .as_ref()
                    .context("Spotify playback is not connected")
                    .map(|player| player.play());
                if result.is_err() {
                    let _ = events.send(BackendEvent::PlaybackSettled);
                }
                result
            }
            BackendCommand::Pause => {
                let result = connection
                    .player
                    .as_ref()
                    .context("Spotify playback is not connected")
                    .map(|player| player.pause());
                if result.is_err() {
                    let _ = events.send(BackendEvent::PlaybackSettled);
                }
                result
            }
            BackendCommand::Next => {
                cancel_radio(&mut radio_task, &mut radio_request_id, &events);
                let next = playback_index
                    .and_then(|index| index.checked_add(1))
                    .filter(|index| *index < playback_tracks.len());
                if let Some(index) = next {
                    let result = load_context_track(
                        &connection.player,
                        &playback_tracks,
                        index,
                        &store,
                        &events,
                    )
                    .await;
                    if result.is_ok() {
                        playback_index = Some(index);
                        playback_position_ms = 0;
                        if let Err(error) = store
                            .set_playback_state(playback_tracks.clone(), index, 0)
                            .await
                        {
                            send_error(&events, error);
                        }
                    } else {
                        let _ = events.send(BackendEvent::PlaybackSettled);
                    }
                    result
                } else {
                    let _ = events.send(BackendEvent::QueueEnded);
                    Ok(())
                }
            }
            BackendCommand::Previous => {
                cancel_radio(&mut radio_task, &mut radio_request_id, &events);
                let previous = playback_index.and_then(|index| index.checked_sub(1));
                if let Some(index) = previous {
                    let result = load_context_track(
                        &connection.player,
                        &playback_tracks,
                        index,
                        &store,
                        &events,
                    )
                    .await;
                    if result.is_ok() {
                        playback_index = Some(index);
                        playback_position_ms = 0;
                        if let Err(error) = store
                            .set_playback_state(playback_tracks.clone(), index, 0)
                            .await
                        {
                            send_error(&events, error);
                        }
                    } else {
                        let _ = events.send(BackendEvent::PlaybackSettled);
                    }
                    result
                } else {
                    let result = connection
                        .player
                        .as_ref()
                        .context("Spotify playback is not connected")
                        .map(|player| player.seek(0));
                    let _ = events.send(BackendEvent::PlaybackSettled);
                    if result.is_ok() {
                        playback_position_ms = 0;
                        if let Err(error) = store.update_playback_position(0).await {
                            send_error(&events, error);
                        }
                    }
                    result
                }
            }
            BackendCommand::Seek(position_ms) => {
                let result = connection
                    .player
                    .as_ref()
                    .context("Spotify playback is not connected")
                    .map(|player| player.seek(position_ms));
                if result.is_ok() {
                    playback_position_ms = position_ms;
                    if let Err(error) = store.update_playback_position(position_ms).await {
                        send_error(&events, error);
                    }
                }
                result
            }
            BackendCommand::SavePlaybackPosition {
                spotify_uri,
                position_ms,
            } => {
                let current_uri = playback_index
                    .and_then(|index| playback_tracks.get(index))
                    .and_then(|track| track.spotify_uri.as_deref());
                if current_uri == Some(spotify_uri.as_str()) {
                    playback_position_ms = position_ms;
                    store.update_playback_position(position_ms).await
                } else {
                    Ok(())
                }
            }
            BackendCommand::SetVolume(volume) => connection
                .player
                .as_ref()
                .context("Spotify playback is not connected")
                .map(|player| player.set_volume(volume)),
            BackendCommand::Shutdown { acknowledged } => {
                for task in catalog_tasks.drain(..) {
                    task.abort();
                }
                catalog.abort_all();
                abort_task(&mut library_reload_task);
                cancel_radio(&mut radio_task, &mut radio_request_id, &events);
                abort_task(&mut authorization_task);
                abort_task(&mut connection.connect);
                if let Some(task) = logout_task.take()
                    && let Err(error) = task.await
                {
                    send_error(&events, error);
                }
                if let Some(task) = favorite_refresh_task.take() {
                    task.abort();
                }
                connection.abort_attempts();
                connection.disconnect();
                return Some(acknowledged);
            }
        };
        if let Err(error) = result {
            send_error(&events, error);
        }
    }
}

async fn wait_for_shutdown(shutdown: &mut tokio::sync::watch::Receiver<bool>) {
    if *shutdown.borrow() {
        return;
    }
    let _ = shutdown.wait_for(|shutdown| *shutdown).await;
}

/// The in-flight catalog lookups.
///
/// Each kind keeps only its newest request: starting another one aborts the
/// previous, so a slow reply cannot outlive the question that asked for it.
#[derive(Default)]
struct CatalogFetches {
    search: Option<tokio::task::JoinHandle<()>>,
    playlist: Option<tokio::task::JoinHandle<()>>,
    artist: Option<tokio::task::JoinHandle<()>>,
    album: Option<tokio::task::JoinHandle<()>>,
}

impl CatalogFetches {
    fn search(&mut self, spotify: Spotify, query: String, respond: Reply<SearchResults>) {
        Self::start(&mut self.search, respond, "Spotify search", async move {
            tokio::try_join!(
                spotify.search_tracks(&query),
                spotify.search_playlists(&query)
            )
        });
    }

    fn playlist(&mut self, spotify: Spotify, playlist: Playlist, respond: Reply<Vec<Track>>) {
        Self::start(
            &mut self.playlist,
            respond,
            "Spotify playlist request",
            async move { spotify.playlist_tracks(&playlist.source_id).await },
        );
    }

    fn artist(&mut self, spotify: Spotify, source_id: String, respond: Reply<ArtistDetails>) {
        Self::start(
            &mut self.artist,
            respond,
            "Spotify artist request",
            async move { spotify.artist(&source_id).await },
        );
    }

    fn album(&mut self, spotify: Spotify, source_id: String, respond: Reply<AlbumDetails>) {
        Self::start(
            &mut self.album,
            respond,
            "Spotify album request",
            async move { spotify.album(&source_id).await },
        );
    }

    fn start<T: Send + 'static>(
        slot: &mut Option<tokio::task::JoinHandle<()>>,
        respond: Reply<T>,
        what: &'static str,
        request: impl Future<Output = Result<T>> + Send + 'static,
    ) {
        abort_task(slot);
        *slot = Some(tokio::spawn(async move {
            let _ = respond.send(run_with_timeout(CATALOG_TIMEOUT_SECONDS, what, request).await);
        }));
    }

    fn abort_all(&mut self) {
        abort_task(&mut self.search);
        abort_task(&mut self.playlist);
        abort_task(&mut self.artist);
        abort_task(&mut self.album);
    }
}

/// The live Spotify playback session and the tasks that keep it alive.
#[derive(Default)]
struct PlaybackConnection {
    player: Option<Playback>,
    observer: Option<tokio::task::JoinHandle<()>>,
    reconnect: Option<tokio::task::JoinHandle<Result<Playback>>>,
    connect: Option<tokio::task::JoinHandle<Result<Playback>>>,
    /// A reconnect is needed and has not succeeded yet.
    reconnect_pending: bool,
    /// The connect in flight is replacing a dropped session rather than starting
    /// a fresh one, so playback must not be restored on top of it.
    connect_restoring: bool,
}

/// Whichever connection attempt finished first.
enum Attempt {
    Reconnected(Result<Result<Playback>, tokio::task::JoinError>),
    Connected(Result<Result<Playback>, tokio::task::JoinError>),
}

impl PlaybackConnection {
    /// Stops the player and the task watching it, leaving both slots empty.
    fn disconnect(&mut self) {
        if let Some(player) = self.player.take() {
            player.stop();
        }
        abort_task(&mut self.observer);
    }

    /// Takes a freshly connected player into use and starts watching it.
    fn adopt(&mut self, player: Playback, events: &UnboundedSender<BackendEvent>) {
        self.observer = Some(observe_playback(&player, events));
        self.player = Some(player);
    }

    /// Abandons whatever connection attempt is in flight.
    fn abort_attempts(&mut self) {
        abort_task(&mut self.reconnect);
        abort_task(&mut self.connect);
    }

    /// Waits for whichever attempt finishes first, and never resolves when none
    /// is in flight. Both slots are borrowed from `self` inside one function
    /// body, which a caller could not do from two separate `select!` arms.
    async fn settled(&mut self) -> Attempt {
        tokio::select! {
            reconnected = finished(&mut self.reconnect) => Attempt::Reconnected(
                reconnected.expect("finished does not resolve on an empty slot"),
            ),
            connected = finished(&mut self.connect) => Attempt::Connected(
                connected.expect("finished does not resolve on an empty slot"),
            ),
        }
    }
}

/// Awaits `task` if there is one, and otherwise never resolves, so a select
/// arm can wait on a slot that may be empty.
async fn finished<T>(
    task: &mut Option<tokio::task::JoinHandle<T>>,
) -> Option<Result<T, tokio::task::JoinError>> {
    match task.as_mut() {
        Some(task) => Some(task.await),
        None => std::future::pending().await,
    }
}

/// Runs `request`, turning a timeout into an error the caller can show.
async fn run_with_timeout<T>(
    seconds: u64,
    what: &str,
    request: impl Future<Output = Result<T>>,
) -> Result<T> {
    tokio::time::timeout(Duration::from_secs(seconds), request)
        .await
        .unwrap_or_else(|_| Err(anyhow!("{what} timed out")))
}

fn abort_task<T>(task: &mut Option<tokio::task::JoinHandle<T>>) {
    if let Some(task) = task.take() {
        task.abort();
    }
}

fn cancel_radio(
    task: &mut Option<RadioTask>,
    request_id: &mut Option<u64>,
    events: &UnboundedSender<BackendEvent>,
) {
    abort_task(task);
    if let Some(request_id) = request_id.take() {
        let _ = events.send(BackendEvent::RadioCancelled { request_id });
    }
}

async fn receive_shutdown_acknowledgment(
    commands: &mut Receiver<BackendCommand>,
) -> Option<StdSender<()>> {
    while let Some(command) = commands.recv().await {
        if let BackendCommand::Shutdown { acknowledged } = command {
            return Some(acknowledged);
        }
    }
    None
}

async fn load_library(spotify: &Spotify) -> Result<(Vec<Track>, Vec<Playlist>)> {
    tokio::try_join!(spotify.liked_tracks(), spotify.playlists())
}

fn spawn_library_load(
    spotify: Spotify,
    store: BlockingStore,
    generation: u64,
    current_generation: Arc<AtomicU64>,
    events: UnboundedSender<BackendEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let profile_load = async {
            let Ok(Ok(profile)) =
                tokio::time::timeout(Duration::from_secs(30), spotify.profile()).await
            else {
                return;
            };
            if current_generation.load(Ordering::Acquire) == generation {
                let _ = events.send(BackendEvent::ProfileLoaded {
                    generation,
                    profile,
                });
            }
        };
        let library_load = async {
            let library =
                tokio::time::timeout(Duration::from_secs(60), load_library(&spotify)).await;
            if current_generation.load(Ordering::Acquire) != generation {
                return;
            }
            match library {
                Ok(Ok((liked_tracks, playlists))) => {
                    match persist_library_cache(
                        &store,
                        liked_tracks,
                        playlists,
                        current_generation.clone(),
                        generation,
                    )
                    .await
                    {
                        Ok(Some((liked_tracks, playlists))) => {
                            let _ = events.send(BackendEvent::LibraryLoaded {
                                generation,
                                liked_tracks,
                                playlists,
                            });
                            let _ = events.send(BackendEvent::CatalogReady { generation });
                        }
                        Ok(None) => {}
                        Err(error) => {
                            let _ = events.send(BackendEvent::CatalogFailed {
                                generation,
                                error: error.to_string(),
                            });
                        }
                    }
                }
                Ok(Err(error)) => {
                    let _ = events.send(BackendEvent::CatalogFailed {
                        generation,
                        error: error.to_string(),
                    });
                }
                Err(_) => {
                    let _ = events.send(BackendEvent::CatalogFailed {
                        generation,
                        error: "Spotify library request timed out".to_owned(),
                    });
                }
            }
        };
        tokio::join!(profile_load, library_load);
    })
}

async fn persist_library_cache(
    store: &BlockingStore,
    liked_tracks: Vec<Track>,
    playlists: Vec<Playlist>,
    current_generation: Arc<AtomicU64>,
    generation: u64,
) -> Result<Option<(Vec<Track>, Vec<Playlist>)>> {
    let liked_tracks = store
        .replace_liked_tracks_if_current(liked_tracks, current_generation, generation)
        .await?;
    Ok(liked_tracks.map(|liked_tracks| (liked_tracks, playlists)))
}

async fn authorize_account(
    mut spotify: Spotify,
    needs_playback: bool,
    playback_credentials_invalidated: bool,
) -> Result<AuthorizationSuccess> {
    let playback_authorization = if needs_playback && playback_credentials_invalidated {
        Some(Playback::prepare_authorization().await?)
    } else {
        None
    };
    let playback_authorization_url = playback_authorization
        .as_ref()
        .map(|authorization| authorization.url().to_owned());
    spotify
        .authorize(playback_authorization_url.as_deref())
        .await?;
    let playback = if needs_playback {
        Some(PlaybackConnectionRequest {
            load_saved_token: !playback_credentials_invalidated,
            authorization: playback_authorization,
        })
    } else {
        None
    };
    Ok(AuthorizationSuccess { playback })
}

async fn logout_account(store: BlockingStore, spotify: Spotify) -> Result<()> {
    let mut errors = Vec::new();
    for result in [
        store.set_oauth_credentials_invalidated(true).await,
        store.set_playback_credentials_invalidated(true).await,
        spotify.logout().await,
        delete_playback_refresh_token().await,
        store.replace_liked_tracks(Vec::new()).await,
        store.clear_playback_state().await,
    ] {
        if let Err(error) = result {
            errors.push(error.to_string());
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow!("logout cleanup failed: {}", errors.join("; ")))
    }
}

async fn connect_playback(
    events: &UnboundedSender<BackendEvent>,
    load_saved_token: bool,
    authorization: Option<PlaybackAuthorization>,
) -> Result<(Playback, tokio::task::JoinHandle<()>)> {
    let player = Playback::connect(load_saved_token, authorization).await?;
    let observer = observe_playback(&player, events);
    let _ = events.send(BackendEvent::PlaybackReady);
    Ok((player, observer))
}

fn observe_playback(
    player: &Playback,
    events: &UnboundedSender<BackendEvent>,
) -> tokio::task::JoinHandle<()> {
    let mut player_events = player.events();
    let event_sender = events.clone();
    tokio::spawn(async move {
        while let Some(event) = player_events.recv().await {
            let event = match event {
                PlayerEvent::Loading { track_id, .. } => Some(BackendEvent::Loading {
                    spotify_uri: track_id.to_string(),
                }),
                PlayerEvent::Playing {
                    track_id,
                    position_ms,
                    ..
                } => {
                    let _ = event_sender.send(BackendEvent::PositionChanged {
                        spotify_uri: track_id.to_string(),
                        position_ms,
                    });
                    Some(BackendEvent::Playing {
                        spotify_uri: track_id.to_string(),
                    })
                }
                PlayerEvent::Paused {
                    track_id,
                    position_ms,
                    ..
                } => {
                    let _ = event_sender.send(BackendEvent::PositionChanged {
                        spotify_uri: track_id.to_string(),
                        position_ms,
                    });
                    Some(BackendEvent::Paused {
                        spotify_uri: track_id.to_string(),
                    })
                }
                PlayerEvent::EndOfTrack { track_id, .. } => Some(BackendEvent::EndOfTrack {
                    spotify_uri: track_id.to_string(),
                }),
                PlayerEvent::Unavailable { track_id, .. } => Some(BackendEvent::TrackFailed {
                    spotify_uri: track_id.to_string(),
                    error: "Spotify cannot play this track".to_owned(),
                }),
                PlayerEvent::PositionChanged {
                    track_id,
                    position_ms,
                    ..
                }
                | PlayerEvent::PositionCorrection {
                    track_id,
                    position_ms,
                    ..
                }
                | PlayerEvent::Seeked {
                    track_id,
                    position_ms,
                    ..
                } => Some(BackendEvent::PositionChanged {
                    spotify_uri: track_id.to_string(),
                    position_ms,
                }),
                _ => None,
            };
            if let Some(event) = event {
                let _ = event_sender.send(event);
            }
        }
    })
}

async fn load_context_track(
    playback: &Option<Playback>,
    tracks: &[Track],
    index: usize,
    store: &BlockingStore,
    events: &UnboundedSender<BackendEvent>,
) -> Result<()> {
    let track = tracks
        .get(index)
        .context("playback track index is out of bounds")?;
    let spotify_uri = track
        .spotify_uri
        .as_deref()
        .context("track has no Spotify playback URI")?;
    let spotify_uri = SpotifyUri::from_uri(spotify_uri).context("invalid Spotify track URI")?;
    let player = playback
        .as_ref()
        .context("Spotify playback is not connected")?;
    send_playback_context(tracks, index, events);
    player.load(spotify_uri, true, 0);
    if let Err(error) = store.add_history(track.clone()).await {
        send_error(events, error);
    } else {
        send_local_state(store, events).await;
    }
    Ok(())
}

fn restore_context_track(
    playback: &Option<Playback>,
    tracks: &[Track],
    index: Option<usize>,
    position_ms: u32,
    playing: bool,
    events: &UnboundedSender<BackendEvent>,
) -> Result<()> {
    let index = index.context("playback context is not available")?;
    let track = tracks
        .get(index)
        .context("playback track index is out of bounds")?;
    let spotify_uri = track
        .spotify_uri
        .as_deref()
        .context("track has no Spotify playback URI")?;
    let spotify_uri = SpotifyUri::from_uri(spotify_uri).context("invalid Spotify track URI")?;
    let player = playback
        .as_ref()
        .context("Spotify playback is not connected")?;
    send_playback_context(tracks, index, events);
    player.load(spotify_uri, playing, position_ms);
    Ok(())
}

fn restore_saved_playback(
    playback: &Option<Playback>,
    tracks: &[Track],
    index: Option<usize>,
    position_ms: u32,
    events: &UnboundedSender<BackendEvent>,
) -> Result<()> {
    if index.is_none() {
        return Ok(());
    }
    restore_context_track(playback, tracks, index, position_ms, false, events)
}

fn send_playback_context(tracks: &[Track], index: usize, events: &UnboundedSender<BackendEvent>) {
    if let Some(current) = tracks.get(index) {
        let _ = events.send(BackendEvent::PlaybackContext {
            current: current.clone(),
            next: tracks.get(index + 1..).unwrap_or_default().to_vec(),
        });
    }
}

fn send_error(events: &UnboundedSender<BackendEvent>, error: impl std::fmt::Display) {
    let _ = events.send(BackendEvent::Error(error.to_string()));
}

fn send_fatal_error(events: &UnboundedSender<BackendEvent>, error: impl std::fmt::Display) {
    let _ = events.send(BackendEvent::FatalError(error.to_string()));
}

async fn spawn_favorite_refresh(
    store: &BlockingStore,
    spotify: Spotify,
    events: &UnboundedSender<BackendEvent>,
) -> Option<tokio::task::JoinHandle<Result<Vec<Track>>>> {
    let favorites = match store.favorites().await {
        Ok(favorites) => favorites,
        Err(error) => {
            send_error(events, error);
            return None;
        }
    };
    let uris = favorites
        .into_iter()
        .filter(favorite_needs_catalog_refresh)
        .map(|track| {
            track
                .spotify_uri
                .unwrap_or_else(|| format!("spotify:track:{}", track.source_id))
        })
        .collect::<Vec<_>>();
    if uris.is_empty() {
        return None;
    }
    Some(tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(60), spotify.resolve_track_uris(&uris))
            .await
            .context("Spotify favorite refresh timed out")?
    }))
}

fn favorite_needs_catalog_refresh(track: &Track) -> bool {
    track.provider == crate::model::Provider::Spotify
        && (!track
            .artists
            .iter()
            .any(|artist| artist.source_id.is_some())
            || track
                .album_ref
                .as_ref()
                .and_then(|album| album.source_id.as_ref())
                .is_none())
}

fn build_radio_context(seed: Track, recommendations: Vec<Track>) -> Result<Vec<Track>> {
    let mut seen = HashSet::from([seed.source_id.clone()]);
    let recommendations = recommendations
        .into_iter()
        .filter(|track| {
            track.spotify_uri.as_deref() != seed.spotify_uri.as_deref()
                && track.is_displayable()
                && seen.insert(track.source_id.clone())
        })
        .collect::<Vec<_>>();
    if recommendations.is_empty() {
        return Err(anyhow!("Spotify track radio returned no playable tracks"));
    }
    let mut tracks = Vec::with_capacity(recommendations.len() + 1);
    tracks.push(seed);
    tracks.extend(recommendations);
    Ok(tracks)
}

async fn send_local_state(store: &BlockingStore, events: &UnboundedSender<BackendEvent>) {
    match store.local_state().await {
        Ok((favorites, pinned_playlists, recently_played)) => {
            let _ = events.send(BackendEvent::LocalStateLoaded {
                favorites,
                pinned_playlists,
                recently_played,
            });
        }
        Err(error) => send_error(events, error),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BackendCommand, BlockingStore, build_radio_context, favorite_needs_catalog_refresh,
        send_command,
    };
    use crate::model::{AlbumRef, ArtistRef, Provider, Track};
    use crate::storage::Store;

    fn favorite() -> Track {
        Track {
            provider: Provider::Spotify,
            source_id: "track-id".to_owned(),
            spotify_uri: Some("spotify:track:track-id".to_owned()),
            isrc: None,
            title: "Track".to_owned(),
            artist: "Artist".to_owned(),
            artists: Vec::new(),
            album: "Album".to_owned(),
            album_ref: None,
            duration_ms: 180_000,
            artwork_url: None,
        }
    }

    #[test]
    fn command_ingress_is_bounded_and_volume_is_coalesced() {
        let (commands, mut command_receiver) = tokio::sync::mpsc::channel(1);
        let (controls, mut control_receiver) = tokio::sync::mpsc::channel(1);
        let (volume, mut volume_receiver) = tokio::sync::watch::channel(0.5);

        assert!(send_command(
            &commands,
            &controls,
            &volume,
            BackendCommand::SetVolume(0.2)
        ));
        assert!(send_command(
            &commands,
            &controls,
            &volume,
            BackendCommand::SetVolume(0.8)
        ));
        assert_eq!(*volume_receiver.borrow_and_update(), 0.8);
        assert!(command_receiver.try_recv().is_err());

        assert!(send_command(
            &commands,
            &controls,
            &volume,
            BackendCommand::Pause
        ));
        assert!(!send_command(
            &commands,
            &controls,
            &volume,
            BackendCommand::Resume
        ));
        assert!(matches!(
            command_receiver.try_recv(),
            Ok(BackendCommand::Pause)
        ));

        assert!(send_command(
            &commands,
            &controls,
            &volume,
            BackendCommand::Logout { generation: 1 }
        ));
        assert!(matches!(
            control_receiver.try_recv(),
            Ok(BackendCommand::Logout { generation: 1 })
        ));
    }

    #[tokio::test]
    async fn blocking_store_runs_operations_off_the_async_thread() {
        let caller = std::thread::current().id();
        let store = BlockingStore::from_store(Store::in_memory().unwrap());

        let worker = store
            .call(|_| Ok(std::thread::current().id()))
            .await
            .unwrap();

        assert_ne!(worker, caller);
    }

    #[tokio::test]
    async fn cancelling_a_store_call_does_not_lose_or_reorder_the_store() {
        let store = BlockingStore::from_store(Store::in_memory().unwrap());
        let first_store = store.clone();
        let first = tokio::spawn(async move {
            first_store
                .call(|store| {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    store.set_spotify_client_id("first")
                })
                .await
        });
        tokio::task::yield_now().await;
        first.abort();

        store
            .call(|store| store.set_spotify_client_id("second"))
            .await
            .unwrap();
        let client_id = store.call(|store| store.spotify_client_id()).await.unwrap();

        assert_eq!(client_id.as_deref(), Some("second"));
    }

    #[test]
    fn only_incomplete_spotify_favorites_need_catalog_refresh() {
        let legacy = favorite();
        assert!(favorite_needs_catalog_refresh(&legacy));

        let mut complete = legacy.clone();
        complete.artists.push(ArtistRef {
            name: "Artist".to_owned(),
            source_id: Some("artist-id".to_owned()),
            spotify_uri: Some("spotify:artist:artist-id".to_owned()),
        });
        complete.album_ref = Some(AlbumRef {
            name: "Album".to_owned(),
            source_id: Some("album-id".to_owned()),
            spotify_uri: Some("spotify:album:album-id".to_owned()),
            artwork_url: None,
        });
        assert!(!favorite_needs_catalog_refresh(&complete));

        let mut non_spotify = legacy;
        non_spotify.provider = Provider::Tidal;
        assert!(!favorite_needs_catalog_refresh(&non_spotify));
    }

    #[test]
    fn radio_context_starts_with_seed_and_deduplicates_recommendations() {
        let seed = favorite();
        let mut recommendation = favorite();
        recommendation.source_id = "recommendation".to_owned();
        recommendation.spotify_uri = Some("spotify:track:recommendation".to_owned());
        recommendation.title = "Recommendation".to_owned();

        let tracks = build_radio_context(
            seed.clone(),
            vec![seed.clone(), recommendation.clone(), recommendation.clone()],
        )
        .unwrap();

        assert_eq!(tracks, vec![seed, recommendation]);
    }

    #[test]
    fn radio_context_rejects_an_empty_recommendation_set() {
        let seed = favorite();

        assert!(build_radio_context(seed.clone(), vec![seed]).is_err());
    }
}
