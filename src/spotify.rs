use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow, bail};
use futures::TryStreamExt;
use keyring::Entry;
use rspotify::{
    AuthCodePkceSpotify, CallbackError, Config, Credentials, OAuth, Token, TokenCallback,
    model::{
        AlbumId, AlbumType, ArtistId, FullAlbum, FullArtist, FullTrack, Id, Image, PlayableItem,
        PlaylistId, SearchResult, SearchType, SimplifiedAlbum, SimplifiedArtist,
        SimplifiedPlaylist, SimplifiedTrack, TrackId,
    },
    prelude::{BaseClient, OAuthClient},
    scopes,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    time::{Duration, timeout},
};

use crate::{
    model::{Album, AlbumRef, Artist, ArtistRef, Playlist, Provider, Track, UserProfile},
    oauth_page::{OAuthStep, success_page},
};

const REDIRECT_URI: &str = "http://127.0.0.1:8888/callback";
const KEYCHAIN_SERVICE: &str = "com.cadence.spotify";
const KEYCHAIN_ACCOUNT: &str = "oauth-token";
const TOKEN_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const LOGGED_OUT_CREDENTIAL: &str = "cadence-logged-out";

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

fn normalized_client_id(client_id: &str) -> Option<String> {
    let client_id = client_id.trim();
    (!client_id.is_empty()).then(|| client_id.to_owned())
}

#[derive(Clone)]
pub struct Spotify {
    client: AuthCodePkceSpotify,
    refresh_lock: Arc<tokio::sync::Mutex<()>>,
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
            token_callback_fn: Arc::new(Some(TokenCallback(Box::new(|token| {
                TokenVault::save(&token)
                    .map_err(|error| CallbackError::CustomizedError(error.to_string()))
            })))),
            token_refreshing: false,
            ..Default::default()
        };
        let client = AuthCodePkceSpotify::with_config(credentials, oauth, config);
        if load_saved_token && let Some(token) = TokenVault::load()? {
            *client.token.lock().await.unwrap() = Some(token);
        }
        Ok(Self {
            client,
            refresh_lock: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    pub async fn is_authorized(&self) -> bool {
        self.client.token.lock().await.unwrap().is_some()
    }

    pub async fn logout(&self) -> Result<()> {
        *self.client.token.lock().await.unwrap() = None;
        TokenVault::delete()
    }

    pub async fn authorize(&mut self, playback_authorization_url: Option<&str>) -> Result<()> {
        if self.is_authorized().await {
            if let Some(url) = playback_authorization_url {
                open::that(url)
                    .context("could not open the Spotify playback authorization page")?;
            }
            return Ok(());
        }
        let listener = TcpListener::bind("127.0.0.1:8888")
            .await
            .context("could not listen for the Spotify authorization callback")?;
        let authorize_url = self.client.get_authorize_url(None)?;
        open::that(&authorize_url).context("could not open the Spotify authorization page")?;

        let (mut stream, _) = timeout(Duration::from_secs(180), listener.accept())
            .await
            .context("Spotify authorization timed out")??;
        let mut request = vec![0; 8192];
        let read = stream.read(&mut request).await?;
        let request = String::from_utf8_lossy(&request[..read]);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .context("Spotify returned an invalid authorization callback")?;
        let response_url = format!("http://127.0.0.1:8888{path}");
        let code = self
            .client
            .parse_response_code(&response_url)
            .context("Spotify did not return an authorization code")?;
        timeout(TOKEN_REQUEST_TIMEOUT, self.client.request_token(&code))
            .await
            .context("Spotify token request timed out")??;

        let body = success_page(OAuthStep::Library, playback_authorization_url);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await?;
        Ok(())
    }

    pub async fn search_tracks(&self, query: &str) -> Result<Vec<Track>> {
        self.refresh_if_needed().await?;
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
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
    }

    pub async fn search_playlists(&self, query: &str) -> Result<Vec<Playlist>> {
        self.refresh_if_needed().await?;
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
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
    }

    pub async fn liked_tracks(&self) -> Result<Vec<Track>> {
        self.refresh_if_needed().await?;
        self.client
            .current_user_saved_tracks(None)
            .map_err(anyhow::Error::from)
            .and_then(|saved| async move {
                if saved.track.is_local || saved.track.id.is_none() {
                    Ok(None)
                } else {
                    convert_track(saved.track).map(|track| track.is_displayable().then_some(track))
                }
            })
            .try_filter_map(|track| async move { Ok(track) })
            .try_collect()
            .await
    }

    pub async fn profile(&self) -> Result<UserProfile> {
        self.refresh_if_needed().await?;
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
    }

    pub async fn playlists(&self) -> Result<Vec<Playlist>> {
        self.refresh_if_needed().await?;
        self.client
            .current_user_playlists()
            .map_err(anyhow::Error::from)
            .map_ok(convert_playlist)
            .try_collect()
            .await
    }

    pub async fn playlist_tracks(&self, source_id: &str) -> Result<Vec<Track>> {
        self.refresh_if_needed().await?;
        let playlist_id = PlaylistId::from_id(source_id)?;
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
    }

    pub async fn artist(&self, source_id: &str) -> Result<(Artist, Vec<Track>, Vec<Album>)> {
        self.refresh_if_needed().await?;
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
            .await
            .unwrap_or_default()
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
        self.refresh_if_needed().await?;
        let album_id = AlbumId::from_id(source_id)?;
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

    pub async fn resolve_track_uris(&self, uris: &[String]) -> Result<Vec<Track>> {
        self.refresh_if_needed().await?;
        let mut tracks = Vec::with_capacity(uris.len());
        for uri in uris {
            let Ok(id) = TrackId::from_uri(uri) else {
                continue;
            };
            let Ok(track) = self.client.track(id, None).await else {
                continue;
            };
            if track.is_local || track.is_playable == Some(false) || track.id.is_none() {
                continue;
            }
            let track = convert_track(track)?;
            if track.is_displayable() {
                tracks.push(track);
            }
        }
        Ok(tracks)
    }

    async fn refresh_if_needed(&self) -> Result<()> {
        let _refresh_guard = self.refresh_lock.lock().await;
        let previous = self.client.token.lock().await.unwrap().clone();
        let Some(previous) = previous else {
            bail!("Spotify is not authorized");
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
        TokenVault::save(&refreshed)?;
        *self.client.token.lock().await.unwrap() = Some(refreshed);
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
    use super::{ClientIdSource, artwork_url, resolve_configuration, valid_client_id};
    use rspotify::model::Image;

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
