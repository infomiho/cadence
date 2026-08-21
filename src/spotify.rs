use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow, bail};
use futures::TryStreamExt;
use keyring::Entry;
use rspotify::{
    AuthCodePkceSpotify, Config, Credentials, OAuth, Token,
    model::{
        AlbumId, AlbumType, ArtistId, FullAlbum, FullArtist, FullTrack, Id, Image, PlayableItem,
        PlaylistId, SavedTrack, SearchResult, SearchType, SimplifiedAlbum, SimplifiedArtist,
        SimplifiedPlaylist, SimplifiedTrack, TrackId,
    },
    prelude::{BaseClient, OAuthClient},
    scopes,
};
use tokio::{
    net::TcpListener,
    time::{Duration, timeout},
};

use crate::{
    credential_worker,
    model::{Album, AlbumRef, Artist, ArtistRef, Playlist, Provider, Track, UserProfile},
    oauth_callback::receive_callback,
    oauth_page::{OAuthStep, success_page},
};

const REDIRECT_URI: &str = "http://127.0.0.1:8888/callback";
const KEYCHAIN_SERVICE: &str = "com.cadence.spotify";
const KEYCHAIN_ACCOUNT: &str = "oauth-token";
const TOKEN_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const LOGGED_OUT_CREDENTIAL: &str = "cadence-logged-out";
/// Spotify's page-size ceiling, used for the head probes.
const HEAD_PAGE_SIZE: u32 = 50;
/// How many single-track lookups overlap: enough to fit a 30-track radio
/// seed inside its timeout, small enough not to burst the rate limit.
const RESOLVE_CONCURRENCY: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientIdSource {
    Environment,
    Saved,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpotifyConfiguration {
    pub client_id: String,
    pub source: ClientIdSource,
}

pub fn resolve_configuration(
    environment_client_id: Option<&str>,
    saved_client_id: Option<&str>,
) -> Option<SpotifyConfiguration> {
    environment_client_id
        .and_then(normalized_client_id)
        .map(|client_id| SpotifyConfiguration {
            client_id,
            source: ClientIdSource::Environment,
        })
        .or_else(|| {
            saved_client_id
                .and_then(normalized_client_id)
                .map(|client_id| SpotifyConfiguration {
                    client_id,
                    source: ClientIdSource::Saved,
                })
        })
}

pub fn valid_client_id(client_id: &str) -> bool {
    let client_id = client_id.trim();
    client_id.len() == 32 && client_id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// What a failed Spotify request means for the caller, recovered from the
/// error chain before it is flattened into text. rspotify has no rate-limit
/// handling of its own; the 429 status and its Retry-After header only
/// survive inside `HttpError::StatusCode`, so this is the one place that
/// reads them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    /// The app exceeded Spotify's rolling rate-limit window. Retry no
    /// earlier than `retry_after` when Spotify sent one.
    RateLimited { retry_after: Option<Duration> },
    /// The token is missing, rejected, or expired beyond refresh: only a new
    /// sign-in can help.
    AuthExpired,
    /// Network trouble or a Spotify-side failure, worth retrying later.
    Transient,
    /// Everything else: bad request, parse failure, programmer error.
    Other,
}

/// The client has no token at all: the listener has to sign in.
#[derive(Debug)]
pub struct NotAuthorized;

impl std::fmt::Display for NotAuthorized {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Spotify is not authorized")
    }
}

impl std::error::Error for NotAuthorized {}

/// A request was refused locally because the rate-limit gate is open: an
/// earlier request got a 429 and its cooldown has not passed yet.
#[derive(Debug)]
pub struct RateLimitActive {
    pub retry_after: Duration,
}

impl std::fmt::Display for RateLimitActive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Spotify is rate limiting requests; retry in {}s",
            self.retry_after.as_secs().max(1)
        )
    }
}

impl std::error::Error for RateLimitActive {}

/// Fallback cooldown growth per consecutive 429 without a Retry-After
/// answer, and its cap.
const RATE_LIMIT_FALLBACK_BASE: Duration = Duration::from_secs(2);
const RATE_LIMIT_FALLBACK_MAX: Duration = Duration::from_secs(60);
/// Random extra hold-off so clients do not retry in lockstep.
const RATE_LIMIT_MAX_JITTER_MS: u64 = 750;

/// Cooldown shared by every Web API request. A 429 opens it for Spotify's
/// Retry-After answer (or an escalating fallback); while open, every request
/// fails fast with [`RateLimitActive`] instead of going to the network.
#[derive(Default)]
struct RateLimitGate {
    state: std::sync::Mutex<GateState>,
}

#[derive(Default)]
struct GateState {
    open_until: Option<std::time::Instant>,
    strikes: u32,
}

impl RateLimitGate {
    fn check(&self) -> Result<()> {
        self.check_at(std::time::Instant::now())
    }

    fn check_at(&self, now: std::time::Instant) -> Result<()> {
        let state = self.state.lock().expect("rate-limit gate lock");
        match state.open_until {
            Some(until) if until > now => Err(RateLimitActive {
                retry_after: until - now,
            }
            .into()),
            _ => Ok(()),
        }
    }

    fn trip(&self, retry_after: Option<Duration>) {
        self.trip_at(retry_after, std::time::Instant::now(), jitter());
    }

    fn trip_at(&self, retry_after: Option<Duration>, now: std::time::Instant, jitter: Duration) {
        let mut state = self.state.lock().expect("rate-limit gate lock");
        state.strikes = state.strikes.saturating_add(1);
        let wait = retry_after.unwrap_or_else(|| fallback_cooldown(state.strikes));
        state.open_until = Some(now + wait + jitter);
    }

    fn reset(&self) {
        self.reset_at(std::time::Instant::now());
    }

    fn reset_at(&self, now: std::time::Instant) {
        let mut state = self.state.lock().expect("rate-limit gate lock");
        // A success that was already in flight when a concurrent 429 opened
        // the cooldown must not clear it, or the strikes it earned.
        let cooling = state.open_until.is_some_and(|until| until > now);
        if !cooling {
            *state = GateState::default();
        }
    }
}

fn fallback_cooldown(strikes: u32) -> Duration {
    let exponent = strikes.saturating_sub(1).min(5);
    (RATE_LIMIT_FALLBACK_BASE * 2u32.pow(exponent)).min(RATE_LIMIT_FALLBACK_MAX)
}

fn jitter() -> Duration {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as u64;
    Duration::from_millis(nanos % (RATE_LIMIT_MAX_JITTER_MS + 1))
}

pub fn classify_error(error: &anyhow::Error) -> ErrorKind {
    for cause in error.chain() {
        if let Some(client_error) = cause.downcast_ref::<rspotify::ClientError>() {
            return classify_client_error(client_error);
        }
        if let Some(cooldown) = cause.downcast_ref::<RateLimitActive>() {
            return ErrorKind::RateLimited {
                retry_after: Some(cooldown.retry_after),
            };
        }
        if cause.downcast_ref::<NotAuthorized>().is_some() {
            return ErrorKind::AuthExpired;
        }
        if cause
            .downcast_ref::<tokio::time::error::Elapsed>()
            .is_some()
        {
            return ErrorKind::Transient;
        }
    }
    ErrorKind::Other
}

fn classify_client_error(error: &rspotify::ClientError) -> ErrorKind {
    use rspotify::http::HttpError;
    match error {
        rspotify::ClientError::InvalidToken => ErrorKind::AuthExpired,
        rspotify::ClientError::Io(_) => ErrorKind::Transient,
        rspotify::ClientError::Http(http) => match http.as_ref() {
            HttpError::Client(_) => ErrorKind::Transient,
            HttpError::StatusCode(response) => match response.status().as_u16() {
                429 => ErrorKind::RateLimited {
                    retry_after: response
                        .headers()
                        .get("retry-after")
                        .and_then(|value| value.to_str().ok())
                        .and_then(|seconds| seconds.parse().ok())
                        .map(Duration::from_secs),
                },
                401 => ErrorKind::AuthExpired,
                500..=599 => ErrorKind::Transient,
                _ => ErrorKind::Other,
            },
        },
        _ => ErrorKind::Other,
    }
}

fn convert_saved_track(saved: SavedTrack) -> Result<Option<Track>> {
    if saved.track.is_local || saved.track.id.is_none() {
        return Ok(None);
    }
    convert_track(saved.track).map(|track| track.is_displayable().then_some(track))
}

fn normalized_client_id(client_id: &str) -> Option<String> {
    let client_id = client_id.trim();
    (!client_id.is_empty()).then(|| client_id.to_owned())
}

#[derive(Clone)]
pub struct Spotify {
    client: AuthCodePkceSpotify,
    refresh_lock: Arc<tokio::sync::Mutex<()>>,
    gate: Arc<RateLimitGate>,
}

impl Spotify {
    pub async fn from_client_id(client_id: &str, load_saved_token: bool) -> Result<Self> {
        let credentials = Credentials::new_pkce(client_id);
        let oauth = OAuth {
            redirect_uri: REDIRECT_URI.to_owned(),
            scopes: scopes!(
                "streaming",
                "user-library-read",
                "playlist-read-private",
                "playlist-read-collaborative",
                "user-read-private"
            ),
            ..Default::default()
        };
        let config = Config {
            token_refreshing: false,
            ..Default::default()
        };
        let client = AuthCodePkceSpotify::with_config(credentials, oauth, config);
        let saved_token = if load_saved_token {
            credential_worker::run(TokenVault::load).await?
        } else {
            None
        };
        if let Some(token) = saved_token {
            *client
                .token
                .lock()
                .await
                .map_err(|_| anyhow!("could not lock Spotify token state"))? = Some(token);
        }
        Ok(Self {
            client,
            refresh_lock: Arc::new(tokio::sync::Mutex::new(())),
            gate: Arc::new(RateLimitGate::default()),
        })
    }

    pub async fn is_authorized(&self) -> Result<bool> {
        Ok(self
            .client
            .token
            .lock()
            .await
            .map_err(|_| anyhow!("could not lock Spotify token state"))?
            .is_some())
    }

    pub async fn logout(&self) -> Result<()> {
        let _refresh_guard = self.refresh_lock.lock().await;
        *self
            .client
            .token
            .lock()
            .await
            .map_err(|_| anyhow!("could not lock Spotify token state"))? = None;
        credential_worker::run(TokenVault::delete).await
    }

    pub async fn authorize(&mut self, playback_authorization_url: Option<&str>) -> Result<()> {
        if self.is_authorized().await? {
            if let Some(url) = playback_authorization_url {
                let url = url.to_owned();
                tokio::task::spawn_blocking(move || open::that(url))
                    .await
                    .context("browser task failed")?
                    .context("could not open the Spotify playback authorization page")?;
            }
            return Ok(());
        }
        let listener = TcpListener::bind("127.0.0.1:8888")
            .await
            .context("could not listen for the Spotify authorization callback")?;
        let authorize_url = self.client.get_authorize_url(None)?;
        tokio::task::spawn_blocking(move || open::that(authorize_url))
            .await
            .context("browser task failed")?
            .context("could not open the Spotify authorization page")?;

        let mut callback = receive_callback(&listener, "/callback")
            .await
            .context("could not receive the Spotify authorization callback")?;
        let code = self
            .client
            .parse_response_code(callback.url().as_str())
            .context("Spotify did not return an authorization code")?;
        timeout(TOKEN_REQUEST_TIMEOUT, self.client.request_token(&code))
            .await
            .context("Spotify token request timed out")??;
        let token = self
            .client
            .token
            .lock()
            .await
            .map_err(|_| anyhow!("could not lock Spotify token state"))?
            .clone()
            .context("Spotify did not return an access token")?;
        credential_worker::run(move || TokenVault::save(&token)).await?;

        let body = success_page(OAuthStep::Library, playback_authorization_url);
        callback.respond_html(&body).await?;
        Ok(())
    }

    pub async fn search_tracks(&self, query: &str) -> Result<Vec<Track>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        self.gated(async {
            let result = self
                .client
                .search(query, SearchType::Track, None, None, Some(10), None)
                .await?;
            match result {
                SearchResult::Tracks(page) => page
                    .items
                    .into_iter()
                    .filter(|track| !track.is_local && track.id.is_some())
                    .map(convert_track)
                    .filter(|track| track.as_ref().is_ok_and(Track::is_displayable))
                    .collect::<Result<Vec<_>>>(),
                _ => bail!("Spotify returned an unexpected search result"),
            }
        })
        .await
    }

    pub async fn search_playlists(&self, query: &str) -> Result<Vec<Playlist>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        self.gated(async {
            let result = self
                .client
                .search(query, SearchType::Playlist, None, None, Some(10), None)
                .await?;
            match result {
                SearchResult::Playlists(page) => {
                    Ok(page.items.into_iter().map(convert_playlist).collect())
                }
                _ => bail!("Spotify returned an unexpected playlist search result"),
            }
        })
        .await
    }

    pub async fn liked_tracks(&self) -> Result<Vec<Track>> {
        self.gated(async {
            self.client
                .current_user_saved_tracks(None)
                .map_err(anyhow::Error::from)
                .and_then(|saved| async move { convert_saved_track(saved) })
                .try_filter_map(|track| async move { Ok(track) })
                .try_collect()
                .await
        })
        .await
    }

    /// First page of the liked collection plus Spotify's total, for change
    /// detection before committing to a full walk.
    pub async fn liked_tracks_head(&self) -> Result<(Vec<Track>, u32)> {
        self.gated(async {
            let page = self
                .client
                .current_user_saved_tracks_manual(None, Some(HEAD_PAGE_SIZE), Some(0))
                .await?;
            let total = page.total;
            let tracks = page
                .items
                .into_iter()
                .filter_map(|saved| convert_saved_track(saved).transpose())
                .collect::<Result<Vec<_>>>()?;
            Ok((tracks, total))
        })
        .await
    }

    pub async fn profile(&self) -> Result<UserProfile> {
        self.gated(async {
            let profile = self.client.current_user().await?;
            Ok(UserProfile {
                display_name: profile
                    .display_name
                    .unwrap_or_else(|| profile.id.id().to_owned()),
                artwork_url: profile
                    .images
                    .and_then(|images| images.into_iter().next())
                    .map(|image| image.url),
            })
        })
        .await
    }

    pub async fn playlists(&self) -> Result<Vec<Playlist>> {
        self.gated(async {
            self.client
                .current_user_playlists()
                .map_err(anyhow::Error::from)
                .map_ok(convert_playlist)
                .try_collect()
                .await
        })
        .await
    }

    /// First page of the playlist list plus Spotify's total, for change
    /// detection before committing to a full walk. Each playlist comes with
    /// its snapshot_id, which Spotify changes on every playlist edit, so
    /// in-playlist changes are visible without fetching any tracks.
    pub async fn playlists_head(&self) -> Result<(Vec<(Playlist, String)>, u32)> {
        self.gated(async {
            let page = self
                .client
                .current_user_playlists_manual(Some(HEAD_PAGE_SIZE), Some(0))
                .await?;
            let total = page.total;
            let playlists = page
                .items
                .into_iter()
                .map(|playlist| {
                    let snapshot = playlist.snapshot_id.clone();
                    (convert_playlist(playlist), snapshot)
                })
                .collect();
            Ok((playlists, total))
        })
        .await
    }

    pub async fn playlist_tracks(&self, source_id: &str) -> Result<Vec<Track>> {
        let playlist_id = PlaylistId::from_id(source_id)?;
        self.gated(async {
            self.client
                .playlist_items(playlist_id, None, None)
                .map_err(anyhow::Error::from)
                .and_then(|item| async move {
                    if item.is_local {
                        return Ok(None);
                    }
                    match item.item {
                        Some(PlayableItem::Track(track)) if track.id.is_some() => {
                            let track = convert_track(track)?;
                            Ok(track.is_displayable().then_some(track))
                        }
                        _ => Ok(None),
                    }
                })
                .try_filter_map(|track| async move { Ok(track) })
                .try_collect()
                .await
        })
        .await
    }

    pub async fn artist(&self, source_id: &str) -> Result<(Artist, Vec<Track>, Vec<Album>)> {
        self.gated(self.artist_request(source_id)).await
    }

    async fn artist_request(&self, source_id: &str) -> Result<(Artist, Vec<Track>, Vec<Album>)> {
        let artist_id = ArtistId::from_id(source_id)?;
        let artist = self
            .client
            .artist(artist_id.as_ref())
            .await
            .context("could not load Spotify artist metadata")?;
        let albums = self
            .client
            .artist_albums_manual(
                artist_id.as_ref(),
                [AlbumType::Album, AlbumType::Single, AlbumType::Compilation],
                None,
                // Spotify Development Mode rejects artist-album limits above 10.
                Some(10),
                None,
            )
            .await
            .context("could not load Spotify artist albums")?
            .items;
        let mut artist = convert_artist(artist);
        let albums = albums
            .into_iter()
            .filter_map(convert_simplified_album)
            .collect::<Vec<_>>();
        if artist.artwork_url.is_none() {
            artist.artwork_url = albums.iter().find_map(|album| album.artwork_url.clone());
        }
        let query = format!("artist:\"{}\"", artist.name.replace('"', ""));
        let tracks = self
            .search_tracks(&query)
            .await?
            .into_iter()
            .filter(|track| {
                track
                    .artists
                    .iter()
                    .any(|candidate| candidate.source_id.as_deref() == Some(source_id))
            })
            .collect();
        Ok((artist, tracks, albums))
    }

    pub async fn album(&self, source_id: &str) -> Result<(Album, Vec<Track>)> {
        let album_id = AlbumId::from_id(source_id)?;
        self.gated(self.album_request(album_id)).await
    }

    async fn album_request(&self, album_id: AlbumId<'_>) -> Result<(Album, Vec<Track>)> {
        let album = self.client.album(album_id.as_ref(), None).await?;
        let album_model = convert_full_album(&album);
        let album_ref = album_ref_from_full(&album);
        let initial_track_count = album.tracks.items.len();
        let tracks = if initial_track_count == album.tracks.total as usize {
            album.tracks.items
        } else {
            self.client
                .album_track(album_id, None)
                .map_err(anyhow::Error::from)
                .try_collect()
                .await?
        };
        let tracks = tracks
            .into_iter()
            .map(|track| convert_simplified_track(track, album_ref.clone()))
            .filter(|track| track.as_ref().is_ok_and(Track::is_displayable))
            .collect::<Result<Vec<_>>>()?;
        Ok((album_model, tracks))
    }

    /// Resolves URIs one track at a time; Spotify removed the batch tracks
    /// endpoint in March 2026. Lookups overlap a few at a time and a missing
    /// or blocked track is skipped. A 429 trips the gate and returns what
    /// resolved so far: a radio that starts with 29 of 30 tracks beats none,
    /// and the favorites repair keeps its partial progress. Auth failure is
    /// fatal: nothing later can succeed.
    pub async fn resolve_track_uris(&self, uris: &[String]) -> Result<Vec<Track>> {
        self.gated(async {
            let ids: Vec<TrackId<'static>> = uris
                .iter()
                .filter_map(|uri| TrackId::from_uri(uri).ok())
                .map(TrackId::into_static)
                .collect();
            let mut tracks = Vec::with_capacity(ids.len());
            'batches: for batch in ids.chunks(RESOLVE_CONCURRENCY) {
                let lookups = batch.iter().map(|id| self.client.track(id.clone(), None));
                for result in futures::future::join_all(lookups).await {
                    let track = match result {
                        Ok(track) => track,
                        Err(error) => {
                            let error = anyhow::Error::from(error);
                            match classify_error(&error) {
                                ErrorKind::RateLimited { retry_after } => {
                                    self.gate.trip(retry_after);
                                    break 'batches;
                                }
                                ErrorKind::AuthExpired => return Err(error),
                                _ => continue,
                            }
                        }
                    };
                    if track.is_local || track.is_playable == Some(false) || track.id.is_none() {
                        continue;
                    }
                    let track = convert_track(track)?;
                    if track.is_displayable() {
                        tracks.push(track);
                    }
                }
            }
            Ok(tracks)
        })
        .await
    }

    /// Runs one Web API request behind the rate-limit gate: fail fast while
    /// a cooldown is open, trip it on a fresh 429, clear it on success. The
    /// token refresh runs inside the match, so its 429s trip the gate too.
    async fn gated<T>(&self, request: impl std::future::Future<Output = Result<T>>) -> Result<T> {
        self.gate.check()?;
        let result = match self.refresh_if_needed().await {
            Ok(()) => request.await,
            Err(error) => Err(error),
        };
        match &result {
            Ok(_) => self.gate.reset(),
            Err(error) => {
                if let ErrorKind::RateLimited { retry_after } = classify_error(error) {
                    self.gate.trip(retry_after);
                }
            }
        }
        result
    }

    async fn refresh_if_needed(&self) -> Result<()> {
        let _refresh_guard = self.refresh_lock.lock().await;
        let previous = self
            .client
            .token
            .lock()
            .await
            .map_err(|_| anyhow!("could not lock Spotify token state"))?
            .clone();
        let Some(previous) = previous else {
            return Err(NotAuthorized.into());
        };
        if !previous.is_expired() {
            return Ok(());
        }

        let mut refreshed = timeout(TOKEN_REQUEST_TIMEOUT, self.client.refetch_token())
            .await
            .context("Spotify token refresh timed out")??
            .context("Spotify did not return a refreshed token")?;
        if refreshed.refresh_token.is_none() {
            refreshed.refresh_token = previous.refresh_token;
        }
        let token_to_save = refreshed.clone();
        credential_worker::run(move || TokenVault::save(&token_to_save)).await?;
        *self
            .client
            .token
            .lock()
            .await
            .map_err(|_| anyhow!("could not lock Spotify token state"))? = Some(refreshed);
        Ok(())
    }
}

fn convert_track(track: FullTrack) -> Result<Track> {
    let id = track.id.context("Spotify track has no ID")?;
    let source_id = id.id().to_owned();
    let duration_ms = u32::try_from(track.duration.num_milliseconds())
        .context("Spotify returned an invalid track duration")?;
    let artists = track
        .artists
        .into_iter()
        .map(convert_artist_ref)
        .collect::<Vec<_>>();
    let album_ref = AlbumRef {
        name: track.album.name.clone(),
        source_id: track.album.id.as_ref().map(|id| id.id().to_owned()),
        spotify_uri: track.album.id.as_ref().map(Id::uri),
        artwork_url: artwork_url(&track.album.images),
    };
    Ok(Track {
        provider: Provider::Spotify,
        source_id,
        spotify_uri: Some(id.uri()),
        isrc: track.external_ids.get("isrc").cloned(),
        title: track.name,
        artist: artists
            .iter()
            .map(|artist| artist.name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        artists,
        album: album_ref.name.clone(),
        album_ref: Some(album_ref),
        duration_ms,
        artwork_url: artwork_url(&track.album.images),
    })
}

fn convert_simplified_track(track: SimplifiedTrack, album: AlbumRef) -> Result<Track> {
    let id = track.id.context("Spotify track has no ID")?;
    let duration_ms = u32::try_from(track.duration.num_milliseconds())
        .context("Spotify returned an invalid track duration")?;
    let artists = track
        .artists
        .into_iter()
        .map(convert_artist_ref)
        .collect::<Vec<_>>();
    Ok(Track {
        provider: Provider::Spotify,
        source_id: id.id().to_owned(),
        spotify_uri: Some(id.uri()),
        isrc: None,
        title: track.name,
        artist: artists
            .iter()
            .map(|artist| artist.name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        artists,
        album: album.name.clone(),
        album_ref: Some(album.clone()),
        duration_ms,
        artwork_url: album.artwork_url,
    })
}

fn convert_artist_ref(artist: SimplifiedArtist) -> ArtistRef {
    let (source_id, spotify_uri) = artist.id.map_or((None, None), |id| {
        (Some(id.id().to_owned()), Some(id.uri()))
    });
    ArtistRef {
        name: artist.name,
        source_id,
        spotify_uri,
    }
}

fn convert_artist(artist: FullArtist) -> Artist {
    Artist {
        provider: Provider::Spotify,
        source_id: artist.id.id().to_owned(),
        spotify_uri: Some(artist.id.uri()),
        name: artist.name,
        artwork_url: artwork_url(&artist.images),
    }
}

fn convert_simplified_album(album: SimplifiedAlbum) -> Option<Album> {
    let id = album.id?;
    Some(Album {
        provider: Provider::Spotify,
        source_id: id.id().to_owned(),
        spotify_uri: Some(id.uri()),
        name: album.name,
        artists: album.artists.into_iter().map(convert_artist_ref).collect(),
        release_date: album.release_date,
        artwork_url: artwork_url(&album.images),
        track_count: None,
    })
}

fn convert_full_album(album: &FullAlbum) -> Album {
    Album {
        provider: Provider::Spotify,
        source_id: album.id.id().to_owned(),
        spotify_uri: Some(album.id.uri()),
        name: album.name.clone(),
        artists: album
            .artists
            .iter()
            .cloned()
            .map(convert_artist_ref)
            .collect(),
        release_date: Some(album.release_date.clone()),
        artwork_url: artwork_url(&album.images),
        track_count: Some(album.tracks.total),
    }
}

fn album_ref_from_full(album: &FullAlbum) -> AlbumRef {
    AlbumRef {
        name: album.name.clone(),
        source_id: Some(album.id.id().to_owned()),
        spotify_uri: Some(album.id.uri()),
        artwork_url: artwork_url(&album.images),
    }
}

fn artwork_url(images: &[Image]) -> Option<String> {
    const TARGET_ARTWORK_SIZE: u32 = 300;

    let image_size = |image: &Image| image.width.or(image.height);
    images
        .iter()
        .filter_map(|image| image_size(image).map(|size| (image, size)))
        .filter(|(_, size)| *size >= TARGET_ARTWORK_SIZE)
        .min_by_key(|(_, size)| *size)
        .map(|(image, _)| image)
        .or_else(|| {
            images
                .iter()
                .filter_map(|image| image_size(image).map(|size| (image, size)))
                .max_by_key(|(_, size)| *size)
                .map(|(image, _)| image)
        })
        .or_else(|| images.iter().find(|image| image_size(image).is_none()))
        .map(|image| image.url.clone())
}

fn convert_playlist(playlist: SimplifiedPlaylist) -> Playlist {
    Playlist {
        provider: Provider::Spotify,
        source_id: playlist.id.id().to_owned(),
        name: playlist.name,
        owner: playlist
            .owner
            .display_name
            .unwrap_or_else(|| "Spotify".to_owned()),
        track_count: playlist.items.total,
        artwork_url: artwork_url(&playlist.images),
    }
}

struct TokenVault;

impl TokenVault {
    fn entry() -> Result<Entry> {
        Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).map_err(Into::into)
    }

    fn load() -> Result<Option<Token>> {
        match Self::entry()?.get_password() {
            Ok(json) if json == LOGGED_OUT_CREDENTIAL => Ok(None),
            Ok(json) => Ok(Some(serde_json::from_str(&json)?)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(anyhow!(error)),
        }
    }

    fn save(token: &Token) -> Result<()> {
        let mut token = token.clone();
        if token.refresh_token.is_none() {
            token.refresh_token = Self::load()?.and_then(|token| token.refresh_token);
        }
        Self::entry()?
            .set_password(&serde_json::to_string(&token)?)
            .map_err(Into::into)
    }

    fn delete() -> Result<()> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(delete_error) => Self::entry()?
                .set_password(LOGGED_OUT_CREDENTIAL)
                .with_context(|| format!("could not invalidate credential after {delete_error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ClientIdSource, Duration, ErrorKind, NotAuthorized, RateLimitGate, anyhow, artwork_url,
        classify_error, resolve_configuration, valid_client_id,
    };
    use rspotify::model::Image;

    fn status_error(status: u16, retry_after: Option<&str>) -> anyhow::Error {
        let mut builder = gpui::http_client::http::Response::builder().status(status);
        if let Some(seconds) = retry_after {
            builder = builder.header("retry-after", seconds);
        }
        let response = oauth2_reqwest::Response::from(builder.body("").unwrap());
        anyhow::Error::from(rspotify::ClientError::Http(Box::new(
            rspotify::http::HttpError::StatusCode(response),
        )))
    }

    #[test]
    fn rate_limited_carries_retry_after() {
        assert_eq!(
            classify_error(&status_error(429, Some("7"))),
            ErrorKind::RateLimited {
                retry_after: Some(Duration::from_secs(7))
            }
        );
        assert_eq!(
            classify_error(&status_error(429, None)),
            ErrorKind::RateLimited { retry_after: None }
        );
    }

    #[test]
    fn auth_failures_classify_as_auth_expired() {
        assert_eq!(
            classify_error(&status_error(401, None)),
            ErrorKind::AuthExpired
        );
        assert_eq!(
            classify_error(&anyhow::Error::from(rspotify::ClientError::InvalidToken)),
            ErrorKind::AuthExpired
        );
        // Context wrapping must not hide the classification.
        assert_eq!(
            classify_error(&anyhow::Error::new(NotAuthorized).context("loading library")),
            ErrorKind::AuthExpired
        );
    }

    #[test]
    fn server_failures_classify_as_transient() {
        assert_eq!(
            classify_error(&status_error(503, None)),
            ErrorKind::Transient
        );
    }

    #[tokio::test]
    async fn timeouts_classify_as_transient() {
        let elapsed = tokio::time::timeout(Duration::from_millis(1), std::future::pending::<()>())
            .await
            .unwrap_err();
        assert_eq!(
            classify_error(&anyhow::Error::new(elapsed).context("request timed out")),
            ErrorKind::Transient
        );
    }

    #[test]
    fn everything_else_classifies_as_other() {
        assert_eq!(classify_error(&anyhow!("boom")), ErrorKind::Other);
        assert_eq!(classify_error(&status_error(404, None)), ErrorKind::Other);
    }

    #[test]
    fn gate_holds_requests_for_the_cooldown() {
        let gate = RateLimitGate::default();
        let now = std::time::Instant::now();
        assert!(gate.check_at(now).is_ok());

        gate.trip_at(Some(Duration::from_secs(5)), now, Duration::ZERO);
        let held = gate.check_at(now + Duration::from_secs(4)).unwrap_err();
        assert!(matches!(
            classify_error(&held),
            ErrorKind::RateLimited {
                retry_after: Some(_)
            }
        ));
        assert!(gate.check_at(now + Duration::from_secs(6)).is_ok());
    }

    #[test]
    fn gate_fallback_escalates_and_resets() {
        let gate = RateLimitGate::default();
        let now = std::time::Instant::now();
        gate.trip_at(None, now, Duration::ZERO);
        assert!(gate.check_at(now + Duration::from_secs(3)).is_ok());

        gate.trip_at(None, now, Duration::ZERO);
        assert!(gate.check_at(now + Duration::from_secs(3)).is_err());
        assert!(gate.check_at(now + Duration::from_secs(5)).is_ok());

        // A success after the cooldown has passed clears the strikes.
        gate.reset_at(now + Duration::from_secs(10));
        gate.trip_at(None, now, Duration::ZERO);
        assert!(gate.check_at(now + Duration::from_secs(3)).is_ok());
    }

    #[test]
    fn gate_success_during_cooldown_does_not_clear_it() {
        let gate = RateLimitGate::default();
        let now = std::time::Instant::now();
        gate.trip_at(Some(Duration::from_secs(30)), now, Duration::ZERO);
        // An in-flight request finishing Ok must not wipe the cooldown a
        // concurrent 429 just opened.
        gate.reset_at(now + Duration::from_secs(1));
        assert!(gate.check_at(now + Duration::from_secs(2)).is_err());
        assert!(gate.check_at(now + Duration::from_secs(31)).is_ok());
    }

    #[test]
    fn gate_fallback_is_capped() {
        assert_eq!(super::fallback_cooldown(1), Duration::from_secs(2));
        assert_eq!(super::fallback_cooldown(3), Duration::from_secs(8));
        assert_eq!(super::fallback_cooldown(100), Duration::from_secs(60));
    }

    fn image(size: Option<u32>, url: &str) -> Image {
        Image {
            height: size,
            width: size,
            url: url.to_owned(),
        }
    }

    #[test]
    fn environment_configuration_takes_precedence_over_saved_configuration() {
        let configuration = resolve_configuration(
            Some(" 0123456789abcdef0123456789abcdef "),
            Some("fedcba9876543210fedcba9876543210"),
        )
        .unwrap();

        assert_eq!(configuration.source, ClientIdSource::Environment);
        assert_eq!(configuration.client_id, "0123456789abcdef0123456789abcdef");
    }

    #[test]
    fn saved_configuration_is_used_when_environment_is_blank() {
        let configuration =
            resolve_configuration(Some("   "), Some("fedcba9876543210fedcba9876543210")).unwrap();

        assert_eq!(configuration.source, ClientIdSource::Saved);
    }

    #[test]
    fn client_id_validation_requires_32_hexadecimal_characters() {
        assert!(valid_client_id("0123456789abcdef0123456789ABCDEF"));
        assert!(valid_client_id(" 0123456789abcdef0123456789abcdef "));
        assert!(!valid_client_id("0527571"));
        assert!(!valid_client_id("z123456789abcdef0123456789abcdef"));
    }

    #[test]
    fn chooses_artwork_closest_to_display_size() {
        let images = [
            image(Some(640), "large"),
            image(Some(64), "small"),
            image(Some(300), "target"),
        ];

        assert_eq!(artwork_url(&images).as_deref(), Some("target"));
    }

    #[test]
    fn falls_back_to_unknown_size_artwork() {
        assert_eq!(
            artwork_url(&[image(None, "unknown")]).as_deref(),
            Some("unknown")
        );
        assert_eq!(artwork_url(&[]), None);
    }

    #[test]
    fn avoids_upscaling_when_larger_artwork_is_available() {
        let images = [image(Some(64), "small"), image(Some(640), "large")];

        assert_eq!(artwork_url(&images).as_deref(), Some("large"));
    }
}
