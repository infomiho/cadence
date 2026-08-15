use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        mpsc::{self, Sender},
    },
    thread,
    time::Duration,
};

use anyhow::{Context as _, Result, anyhow};
use librespot::{core::SpotifyUri, playback::player::PlayerEvent};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    model::{Album, Artist, Playlist, Track, UserProfile},
    playback::{Playback, delete_playback_refresh_token},
    spotify::Spotify,
    storage::Store,
};

#[derive(Debug)]
pub enum BackendCommand {
    Authenticate {
        generation: u64,
    },
    Logout {
        generation: u64,
    },
    SearchCatalog {
        request_id: u64,
        query: String,
    },
    LoadPlaylist {
        request_id: u64,
        playlist: Playlist,
    },
    LoadArtist {
        request_id: u64,
        source_id: String,
    },
    LoadAlbum {
        request_id: u64,
        source_id: String,
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
        acknowledged: Sender<()>,
    },
}

#[derive(Debug)]
pub enum BackendEvent {
    SetupRequired,
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
    SearchResults {
        generation: u64,
        request_id: u64,
        tracks: Vec<Track>,
        playlists: Vec<Playlist>,
    },
    SearchFailed {
        generation: u64,
        request_id: u64,
        error: String,
    },
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
    PlaylistLoaded {
        generation: u64,
        request_id: u64,
        playlist: Playlist,
        tracks: Vec<Track>,
    },
    PlaylistFailed {
        generation: u64,
        request_id: u64,
        source_id: String,
        error: String,
    },
    ArtistLoaded {
        generation: u64,
        request_id: u64,
        source_id: String,
        artist: Artist,
        tracks: Vec<Track>,
        albums: Vec<Album>,
    },
    ArtistFailed {
        generation: u64,
        request_id: u64,
        source_id: String,
        error: String,
    },
    AlbumLoaded {
        generation: u64,
        request_id: u64,
        source_id: String,
        album: Album,
        tracks: Vec<Track>,
    },
    AlbumFailed {
        generation: u64,
        request_id: u64,
        source_id: String,
        error: String,
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
    Error(String),
}

pub struct Backend {
    commands: UnboundedSender<BackendCommand>,
    shutdown: tokio::sync::watch::Sender<bool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Backend {
    pub fn start() -> (Self, UnboundedReceiver<BackendEvent>) {
        let (commands, command_receiver) = tokio::sync::mpsc::unbounded_channel();
        let (event_sender, events) = tokio::sync::mpsc::unbounded_channel();
        let (shutdown, shutdown_receiver) = tokio::sync::watch::channel(false);
        let thread = thread::Builder::new()
            .name("cadence-backend".to_owned())
            .spawn(move || {
                let runtime = tokio::runtime::Runtime::new().expect("could not start Tokio");
                let acknowledged =
                    runtime.block_on(run(command_receiver, event_sender, shutdown_receiver));
                runtime.shutdown_timeout(Duration::from_secs(1));
                if let Some(acknowledged) = acknowledged {
                    let _ = acknowledged.send(());
                }
            })
            .expect("could not start the Cadence backend");
        (
            Self {
                commands,
                shutdown,
                thread: Some(thread),
            },
            events,
        )
    }

    pub fn send(&self, command: BackendCommand) -> bool {
        self.commands.send(command).is_ok()
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        let (acknowledged, acknowledgment) = mpsc::channel();
        let _ = self.shutdown.send(true);
        let sent = self
            .commands
            .send(BackendCommand::Shutdown { acknowledged })
            .is_ok();
        let stopped = sent && acknowledgment.recv_timeout(Duration::from_secs(2)).is_ok();
        if stopped && let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn run(
    mut commands: UnboundedReceiver<BackendCommand>,
    events: UnboundedSender<BackendEvent>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Option<Sender<()>> {
    let mut store = match Store::open_default() {
        Ok(store) => store,
        Err(error) => {
            send_error(&events, error);
            return None;
        }
    };
    send_local_state(&store, &events);
    let playback_snapshot = match store.playback_state() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            send_error(&events, error);
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
    let mut spotify = match tokio::select! {
        result = Spotify::from_environment() => result,
        _ = wait_for_shutdown(&mut shutdown) => return receive_shutdown_acknowledgment(&mut commands).await,
    } {
        Ok(spotify) => spotify,
        Err(error) if error.to_string().contains("SPOTIFY_CLIENT_ID") => {
            let _ = events.send(BackendEvent::SetupRequired);
            while let Some(command) = commands.recv().await {
                if let BackendCommand::Shutdown { acknowledged } = command {
                    return Some(acknowledged);
                }
            }
            return None;
        }
        Err(error) => {
            send_error(&events, error);
            return None;
        }
    };
    let mut playback = None;
    let mut playback_tracks = playback_snapshot
        .as_ref()
        .map(|snapshot| snapshot.tracks.clone())
        .unwrap_or_default();
    let mut playback_index = playback_snapshot.as_ref().map(|snapshot| snapshot.index);
    let mut playback_position_ms = playback_snapshot
        .as_ref()
        .map_or(0, |snapshot| snapshot.position_ms);
    let mut playback_reconnect_pending = false;
    let mut account_generation = 0;
    let catalog_generation = Arc::new(Mutex::new(account_generation));
    let mut playback_reconnect_task: Option<tokio::task::JoinHandle<Result<Playback>>> = None;
    let mut playback_observer: Option<tokio::task::JoinHandle<()>> = None;
    let mut favorite_refresh_task = None;
    let mut catalog_tasks = Vec::new();
    let mut search_task = None;
    let mut playlist_task = None;
    let mut artist_task = None;
    let mut album_task = None;
    let is_authorized = tokio::select! {
        authorized = spotify.is_authorized() => authorized,
        _ = wait_for_shutdown(&mut shutdown) => return receive_shutdown_acknowledgment(&mut commands).await,
    };
    if is_authorized {
        match store.liked_tracks() {
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
            account_generation,
            catalog_generation.clone(),
            events.clone(),
        ));
        favorite_refresh_task = spawn_favorite_refresh(&store, spotify.clone(), &events);
        let connection = tokio::select! {
            result = connect_playback(&events) => result,
            _ = wait_for_shutdown(&mut shutdown) => {
                for task in catalog_tasks.drain(..) {
                    task.abort();
                }
                abort_task(&mut search_task);
                abort_task(&mut playlist_task);
                abort_task(&mut artist_task);
                abort_task(&mut album_task);
                if let Some(task) = favorite_refresh_task.take() {
                    task.abort();
                }
                return receive_shutdown_acknowledgment(&mut commands).await;
            },
        };
        match connection {
            Ok((player, observer)) => {
                playback = Some(player);
                playback_observer = Some(observer);
                if let Err(error) = restore_saved_playback(
                    &playback,
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
            command = commands.recv() => {
                command?
            }
            _ = playback_health.tick() => {
                if playback_reconnect_pending
                    || playback.as_ref().is_some_and(|player| !player.is_connected())
                {
                    playback_reconnect_pending = true;
                    if playback_reconnect_task.is_none() {
                        if let Some(player) = playback.take() {
                            player.stop();
                        }
                        if let Some(observer) = playback_observer.take() {
                            observer.abort();
                        }
                        let _ = events.send(BackendEvent::PlaybackReconnecting);
                        playback_reconnect_task = Some(tokio::spawn(async {
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
            reconnect = async {
                match playback_reconnect_task.as_mut() {
                    Some(task) => Some(task.await),
                    None => std::future::pending().await,
                }
            } => {
                playback_reconnect_task = None;
                match reconnect {
                    Some(Ok(Ok(player))) => {
                        playback_observer = Some(observe_playback(&player, &events));
                        playback = Some(player);
                        playback_reconnect_pending = false;
                        let _ = events.send(BackendEvent::PlaybackReconnected);
                    }
                    Some(Ok(Err(error))) => {
                        let _ = events.send(BackendEvent::PlaybackFailed(format!(
                            "Spotify playback disconnected; reconnecting: {error}"
                        )));
                    }
                    Some(Err(error)) => send_error(&events, error),
                    None => {}
                }
                continue;
            }
            refreshed = async {
                match favorite_refresh_task.as_mut() {
                    Some(task) => Some(task.await),
                    None => std::future::pending().await,
                }
            } => {
                favorite_refresh_task = None;
                match refreshed {
                    Some(Ok(Ok(tracks))) => {
                        let mut changed = false;
                        for track in tracks {
                            match store.set_favorite(&track, true) {
                                Ok(()) => changed = true,
                                Err(error) => send_error(&events, error),
                            }
                        }
                        if changed {
                            send_local_state(&store, &events);
                        }
                    }
                    Some(Ok(Err(error))) => send_error(&events, error),
                    Some(Err(error)) => send_error(&events, error),
                    None => {}
                }
                continue;
            }
        };
        catalog_tasks.retain(|task| !task.is_finished());
        let result = match command {
            BackendCommand::Authenticate { generation } => match tokio::select! {
                result = spotify.authorize() => result,
                _ = wait_for_shutdown(&mut shutdown) => continue,
            } {
                Ok(()) => {
                    account_generation = generation;
                    *catalog_generation
                        .lock()
                        .expect("catalog generation poisoned") = generation;
                    for task in catalog_tasks.drain(..) {
                        task.abort();
                    }
                    abort_task(&mut search_task);
                    abort_task(&mut playlist_task);
                    abort_task(&mut artist_task);
                    abort_task(&mut album_task);
                    if let Some(task) = favorite_refresh_task.take() {
                        task.abort();
                    }
                    catalog_tasks.push(spawn_library_load(
                        spotify.clone(),
                        account_generation,
                        catalog_generation.clone(),
                        events.clone(),
                    ));
                    if favorite_refresh_task.is_none() {
                        favorite_refresh_task =
                            spawn_favorite_refresh(&store, spotify.clone(), &events);
                    }
                    if playback
                        .as_ref()
                        .is_none_or(|player| !player.is_connected())
                    {
                        let restoring_playback = playback_reconnect_pending;
                        if let Some(task) = playback_reconnect_task.take() {
                            task.abort();
                        }
                        playback_reconnect_pending = false;
                        if let Some(player) = playback.take() {
                            player.stop();
                        }
                        if let Some(observer) = playback_observer.take() {
                            observer.abort();
                        }
                        let connection = tokio::select! {
                            result = connect_playback(&events) => result,
                            _ = wait_for_shutdown(&mut shutdown) => continue,
                        };
                        match connection {
                            Ok((player, observer)) => {
                                playback = Some(player);
                                playback_observer = Some(observer);
                                if restoring_playback {
                                    let _ = events.send(BackendEvent::PlaybackReconnected);
                                } else if let Err(error) = restore_saved_playback(
                                    &playback,
                                    &playback_tracks,
                                    playback_index,
                                    playback_position_ms,
                                    &events,
                                ) {
                                    send_error(&events, error);
                                }
                            }
                            Err(error) => {
                                let _ =
                                    events.send(BackendEvent::PlaybackFailed(error.to_string()));
                            }
                        }
                    }
                    Ok(())
                }
                Err(error) => {
                    let _ = events.send(BackendEvent::AuthorizationFailed(error.to_string()));
                    Ok(())
                }
            },
            BackendCommand::Logout { generation } => {
                account_generation = generation;
                *catalog_generation
                    .lock()
                    .expect("catalog generation poisoned") = generation;
                for task in catalog_tasks.drain(..) {
                    task.abort();
                }
                abort_task(&mut search_task);
                abort_task(&mut playlist_task);
                abort_task(&mut artist_task);
                abort_task(&mut album_task);
                if let Some(task) = favorite_refresh_task.take() {
                    task.abort();
                }
                if let Some(player) = playback.take() {
                    player.stop();
                }
                if let Some(observer) = playback_observer.take() {
                    observer.abort();
                }
                playback_tracks.clear();
                playback_index = None;
                playback_reconnect_pending = false;
                if let Some(task) = playback_reconnect_task.take() {
                    task.abort();
                }
                let mut errors = Vec::new();
                let logout = tokio::select! {
                    result = spotify.logout() => Some(result),
                    _ = wait_for_shutdown(&mut shutdown) => None,
                };
                if let Some(Err(error)) = logout {
                    errors.push(error.to_string());
                }
                if let Err(error) = delete_playback_refresh_token() {
                    errors.push(error.to_string());
                }
                if let Err(error) = store.replace_liked_tracks(&[]) {
                    errors.push(error.to_string());
                }
                if let Err(error) = store.clear_playback_state() {
                    errors.push(error.to_string());
                }
                let _ = events.send(BackendEvent::LoggedOut);
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(anyhow!("logout cleanup failed: {}", errors.join("; ")))
                }
            }
            BackendCommand::SearchCatalog { request_id, query } => {
                if let Some(task) = search_task.take() {
                    task.abort();
                }
                let spotify = spotify.clone();
                let events = events.clone();
                let generation = account_generation;
                search_task = Some(tokio::spawn(async move {
                    match tokio::time::timeout(Duration::from_secs(30), async {
                        tokio::try_join!(
                            spotify.search_tracks(&query),
                            spotify.search_playlists(&query)
                        )
                    })
                    .await
                    {
                        Ok(Ok((tracks, playlists))) => {
                            let _ = events.send(BackendEvent::SearchResults {
                                generation,
                                request_id,
                                tracks,
                                playlists,
                            });
                        }
                        Ok(Err(error)) => {
                            let _ = events.send(BackendEvent::SearchFailed {
                                generation,
                                request_id,
                                error: error.to_string(),
                            });
                        }
                        Err(_) => {
                            let _ = events.send(BackendEvent::SearchFailed {
                                generation,
                                request_id,
                                error: "Spotify search timed out".to_owned(),
                            });
                        }
                    }
                }));
                Ok(())
            }
            BackendCommand::LoadPlaylist {
                request_id,
                playlist,
            } => {
                if let Some(task) = playlist_task.take() {
                    task.abort();
                }
                let spotify = spotify.clone();
                let events = events.clone();
                let generation = account_generation;
                playlist_task = Some(tokio::spawn(async move {
                    match tokio::time::timeout(
                        Duration::from_secs(30),
                        spotify.playlist_tracks(&playlist.source_id),
                    )
                    .await
                    {
                        Ok(Ok(tracks)) => {
                            let _ = events.send(BackendEvent::PlaylistLoaded {
                                generation,
                                request_id,
                                playlist,
                                tracks,
                            });
                        }
                        Ok(Err(error)) => {
                            let _ = events.send(BackendEvent::PlaylistFailed {
                                generation,
                                request_id,
                                source_id: playlist.source_id,
                                error: error.to_string(),
                            });
                        }
                        Err(_) => {
                            let _ = events.send(BackendEvent::PlaylistFailed {
                                generation,
                                request_id,
                                source_id: playlist.source_id,
                                error: "Spotify playlist request timed out".to_owned(),
                            });
                        }
                    }
                }));
                Ok(())
            }
            BackendCommand::LoadArtist {
                request_id,
                source_id,
            } => {
                if let Some(task) = artist_task.take() {
                    task.abort();
                }
                let spotify = spotify.clone();
                let events = events.clone();
                let generation = account_generation;
                artist_task = Some(tokio::spawn(async move {
                    match tokio::time::timeout(Duration::from_secs(30), spotify.artist(&source_id))
                        .await
                    {
                        Ok(Ok((artist, tracks, albums))) => {
                            let _ = events.send(BackendEvent::ArtistLoaded {
                                generation,
                                request_id,
                                source_id,
                                artist,
                                tracks,
                                albums,
                            });
                        }
                        Ok(Err(error)) => {
                            let _ = events.send(BackendEvent::ArtistFailed {
                                generation,
                                request_id,
                                source_id,
                                error: format!("{error:#}"),
                            });
                        }
                        Err(_) => {
                            let _ = events.send(BackendEvent::ArtistFailed {
                                generation,
                                request_id,
                                source_id,
                                error: "Spotify artist request timed out".to_owned(),
                            });
                        }
                    }
                }));
                Ok(())
            }
            BackendCommand::LoadAlbum {
                request_id,
                source_id,
            } => {
                if let Some(task) = album_task.take() {
                    task.abort();
                }
                let spotify = spotify.clone();
                let events = events.clone();
                let generation = account_generation;
                album_task = Some(tokio::spawn(async move {
                    match tokio::time::timeout(Duration::from_secs(30), spotify.album(&source_id))
                        .await
                    {
                        Ok(Ok((album, tracks))) => {
                            let _ = events.send(BackendEvent::AlbumLoaded {
                                generation,
                                request_id,
                                source_id,
                                album,
                                tracks,
                            });
                        }
                        Ok(Err(error)) => {
                            let _ = events.send(BackendEvent::AlbumFailed {
                                generation,
                                request_id,
                                source_id,
                                error: error.to_string(),
                            });
                        }
                        Err(_) => {
                            let _ = events.send(BackendEvent::AlbumFailed {
                                generation,
                                request_id,
                                source_id,
                                error: "Spotify album request timed out".to_owned(),
                            });
                        }
                    }
                }));
                Ok(())
            }
            BackendCommand::StartRadio { request_id, seed } => {
                let result = tokio::select! {
                    _ = wait_for_shutdown(&mut shutdown) => continue,
                    result = tokio::time::timeout(Duration::from_secs(20), async {
                    let player = playback
                        .as_ref()
                        .context("Spotify playback is not connected")?;
                    let seed_uri = seed
                        .spotify_uri
                        .as_deref()
                        .context("radio seed has no Spotify track URI")?;
                    let uris = player.radio_track_uris(seed_uri).await?;
                    let recommendations = spotify.resolve_track_uris(&uris).await?;
                    let tracks = build_radio_context(seed.clone(), recommendations)?;
                    load_context_track(&playback, &tracks, 0, &store, &events)?;
                    Ok(tracks)
                    }) => match result {
                        Ok(result) => result,
                        Err(_) => Err(anyhow!("Spotify track radio timed out")),
                    },
                };
                match result {
                    Ok(tracks) => {
                        playback_tracks = tracks;
                        playback_index = Some(0);
                        playback_position_ms = 0;
                        if let Err(error) = store.set_playback_state(&playback_tracks, 0, 0) {
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
                Ok(())
            }
            BackendCommand::PlayContext { tracks, index } => {
                let spotify_uri = tracks
                    .get(index)
                    .and_then(|track| track.spotify_uri.clone())
                    .unwrap_or_default();
                match load_context_track(&playback, &tracks, index, &store, &events) {
                    Ok(()) => {
                        playback_tracks = tracks;
                        playback_index = Some(index);
                        playback_position_ms = 0;
                        if let Err(error) = store.set_playback_state(&playback_tracks, index, 0) {
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
                    match store.set_playback_state(&updated_tracks, index, playback_position_ms) {
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
                    match store.set_playback_state(&updated_tracks, index, playback_position_ms) {
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
                    &playback,
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
                    if let Err(error) = store.update_playback_position(position_ms) {
                        send_error(&events, error);
                    }
                }
                result
            }
            BackendCommand::SetFavorite { track, favorite } => {
                let result = store.set_favorite(&track, favorite);
                if result.is_ok() {
                    send_local_state(&store, &events);
                }
                result
            }
            BackendCommand::SetPlaylistPinned { playlist, pinned } => {
                let result = store.set_playlist_pinned(&playlist, pinned);
                if result.is_ok() {
                    send_local_state(&store, &events);
                }
                result
            }
            BackendCommand::Resume => {
                let result = playback
                    .as_ref()
                    .context("Spotify playback is not connected")
                    .map(|player| player.play());
                if result.is_err() {
                    let _ = events.send(BackendEvent::PlaybackSettled);
                }
                result
            }
            BackendCommand::Pause => {
                let result = playback
                    .as_ref()
                    .context("Spotify playback is not connected")
                    .map(|player| player.pause());
                if result.is_err() {
                    let _ = events.send(BackendEvent::PlaybackSettled);
                }
                result
            }
            BackendCommand::Next => {
                let next = playback_index
                    .and_then(|index| index.checked_add(1))
                    .filter(|index| *index < playback_tracks.len());
                if let Some(index) = next {
                    let result =
                        load_context_track(&playback, &playback_tracks, index, &store, &events);
                    if result.is_ok() {
                        playback_index = Some(index);
                        playback_position_ms = 0;
                        if let Err(error) = store.set_playback_state(&playback_tracks, index, 0) {
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
                let previous = playback_index.and_then(|index| index.checked_sub(1));
                if let Some(index) = previous {
                    let result =
                        load_context_track(&playback, &playback_tracks, index, &store, &events);
                    if result.is_ok() {
                        playback_index = Some(index);
                        playback_position_ms = 0;
                        if let Err(error) = store.set_playback_state(&playback_tracks, index, 0) {
                            send_error(&events, error);
                        }
                    } else {
                        let _ = events.send(BackendEvent::PlaybackSettled);
                    }
                    result
                } else {
                    let result = playback
                        .as_ref()
                        .context("Spotify playback is not connected")
                        .map(|player| player.seek(0));
                    let _ = events.send(BackendEvent::PlaybackSettled);
                    if result.is_ok() {
                        playback_position_ms = 0;
                        if let Err(error) = store.update_playback_position(0) {
                            send_error(&events, error);
                        }
                    }
                    result
                }
            }
            BackendCommand::Seek(position_ms) => {
                let result = playback
                    .as_ref()
                    .context("Spotify playback is not connected")
                    .map(|player| player.seek(position_ms));
                if result.is_ok() {
                    playback_position_ms = position_ms;
                    if let Err(error) = store.update_playback_position(position_ms) {
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
                    store.update_playback_position(position_ms)
                } else {
                    Ok(())
                }
            }
            BackendCommand::SetVolume(volume) => playback
                .as_ref()
                .context("Spotify playback is not connected")
                .map(|player| player.set_volume(volume)),
            BackendCommand::Shutdown { acknowledged } => {
                for task in catalog_tasks.drain(..) {
                    task.abort();
                }
                abort_task(&mut search_task);
                abort_task(&mut playlist_task);
                abort_task(&mut artist_task);
                abort_task(&mut album_task);
                if let Some(task) = favorite_refresh_task.take() {
                    task.abort();
                }
                if let Some(task) = playback_reconnect_task.take() {
                    task.abort();
                }
                if let Some(observer) = playback_observer.take() {
                    observer.abort();
                }
                if let Some(player) = playback {
                    player.stop();
                }
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

fn abort_task<T>(task: &mut Option<tokio::task::JoinHandle<T>>) {
    if let Some(task) = task.take() {
        task.abort();
    }
}

async fn receive_shutdown_acknowledgment(
    commands: &mut UnboundedReceiver<BackendCommand>,
) -> Option<Sender<()>> {
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
    generation: u64,
    current_generation: Arc<Mutex<u64>>,
    events: UnboundedSender<BackendEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let profile_load = async {
            let Ok(Ok(profile)) =
                tokio::time::timeout(Duration::from_secs(30), spotify.profile()).await
            else {
                return;
            };
            let current_generation = current_generation
                .lock()
                .expect("catalog generation poisoned");
            if *current_generation == generation {
                let _ = events.send(BackendEvent::ProfileLoaded {
                    generation,
                    profile,
                });
            }
        };
        let library_load = async {
            let library =
                tokio::time::timeout(Duration::from_secs(60), load_library(&spotify)).await;
            let current_generation = current_generation
                .lock()
                .expect("catalog generation poisoned");
            if *current_generation != generation {
                return;
            }
            match library {
                Ok(Ok((liked_tracks, playlists))) => match Store::open_default()
                    .and_then(|mut store| store.replace_liked_tracks(&liked_tracks))
                {
                    Ok(()) => {
                        let _ = events.send(BackendEvent::LibraryLoaded {
                            generation,
                            liked_tracks,
                            playlists,
                        });
                        let _ = events.send(BackendEvent::CatalogReady { generation });
                    }
                    Err(error) => {
                        let _ = events.send(BackendEvent::CatalogFailed {
                            generation,
                            error: error.to_string(),
                        });
                    }
                },
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

async fn connect_playback(
    events: &UnboundedSender<BackendEvent>,
) -> Result<(Playback, tokio::task::JoinHandle<()>)> {
    let player = Playback::connect().await?;
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

fn load_context_track(
    playback: &Option<Playback>,
    tracks: &[Track],
    index: usize,
    store: &Store,
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
    if let Err(error) = store.add_history(track) {
        send_error(events, error);
    } else {
        send_local_state(store, events);
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

fn spawn_favorite_refresh(
    store: &Store,
    spotify: Spotify,
    events: &UnboundedSender<BackendEvent>,
) -> Option<tokio::task::JoinHandle<Result<Vec<Track>>>> {
    let favorites = match store.favorites() {
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

fn send_local_state(store: &Store, events: &UnboundedSender<BackendEvent>) {
    match (
        store.favorites(),
        store.pinned_playlists(),
        store.recent_tracks(100),
    ) {
        (Ok(favorites), Ok(pinned_playlists), Ok(recently_played)) => {
            let _ = events.send(BackendEvent::LocalStateLoaded {
                favorites,
                pinned_playlists,
                recently_played,
            });
        }
        (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => send_error(events, error),
    }
}

#[cfg(test)]
mod tests {
    use super::{build_radio_context, favorite_needs_catalog_refresh};
    use crate::model::{AlbumRef, ArtistRef, Provider, Track};

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
