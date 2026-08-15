use std::{collections::HashSet, sync::Arc, time::Duration};

use anyhow::{Context as _, Result, anyhow};
use keyring::Entry;
use librespot::{
    core::{SpotifyUri, authentication::Credentials, config::SessionConfig, session::Session},
    oauth::OAuthClientBuilder,
    playback::{
        config::{AudioFormat, PlayerConfig, VolumeCtrl},
        mixer::{self, Mixer, MixerConfig},
        player::{Player, PlayerEventChannel},
    },
};

use crate::audio::low_latency_sdl_sink;

const PLAYBACK_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";
const PLAYBACK_REDIRECT_URI: &str = "http://127.0.0.1:8898/login";
const KEYCHAIN_SERVICE: &str = "com.cadence.spotify";
const KEYCHAIN_ACCOUNT: &str = "playback-refresh-token";

pub struct Playback {
    player: Arc<Player>,
    mixer: Arc<dyn Mixer>,
    session: Session,
}

impl Playback {
    pub async fn connect() -> Result<Self> {
        let oauth =
            OAuthClientBuilder::new(PLAYBACK_CLIENT_ID, PLAYBACK_REDIRECT_URI, vec!["streaming"])
                .open_in_browser()
                .with_custom_message("Cadence playback is connected. You can close this tab.")
                .build()
                .context("could not configure librespot authorization")?;
        let token = match playback_refresh_token().context(
            "could not read playback credentials from Keychain; choose Always Allow when macOS asks",
        )? {
            Some(refresh_token) => {
                let mut token = match oauth.refresh_token_async(&refresh_token).await {
                    Ok(token) => token,
                    Err(_) => oauth
                        .get_access_token_async()
                        .await
                        .context("could not reauthorize librespot playback")?,
                };
                if token.refresh_token.is_empty() {
                    token.refresh_token = refresh_token;
                }
                token
            }
            None => oauth
                .get_access_token_async()
                .await
                .context("could not authorize librespot playback")?,
        };
        if token.refresh_token.is_empty() {
            return Err(anyhow!("Spotify returned an empty playback refresh token"));
        }
        save_playback_refresh_token(&token.refresh_token)?;
        Self::connect_with_access_token(token.access_token).await
    }

    pub async fn reconnect() -> Result<Self> {
        let refresh_token =
            playback_refresh_token()?.context("Spotify playback credentials are not available")?;
        let oauth =
            OAuthClientBuilder::new(PLAYBACK_CLIENT_ID, PLAYBACK_REDIRECT_URI, vec!["streaming"])
                .build()
                .context("could not configure librespot reconnection")?;
        let mut token = oauth
            .refresh_token_async(&refresh_token)
            .await
            .context("could not refresh librespot playback credentials")?;
        if token.refresh_token.is_empty() {
            token.refresh_token = refresh_token;
        }
        save_playback_refresh_token(&token.refresh_token)?;
        Self::connect_with_access_token(token.access_token).await
    }

    async fn connect_with_access_token(access_token: String) -> Result<Self> {
        let session = Session::new(SessionConfig::default(), None);
        session
            .connect(Credentials::with_access_token(access_token), false)
            .await
            .context("librespot could not connect to Spotify")?;
        let mixer = mixer::find(None).context("no supported audio mixer is available")?(
            MixerConfig::default(),
        )?;
        let volume = mixer.get_soft_volume();
        let player_config = PlayerConfig {
            position_update_interval: Some(Duration::from_millis(250)),
            ..PlayerConfig::default()
        };
        let player = Player::new(player_config, session.clone(), volume, move || {
            low_latency_sdl_sink(None, AudioFormat::default())
        });
        Ok(Self {
            player,
            mixer,
            session,
        })
    }

    pub fn load(&self, spotify_uri: SpotifyUri, playing: bool, position_ms: u32) {
        self.player.load(spotify_uri, playing, position_ms);
    }

    pub fn play(&self) {
        self.player.play();
    }

    pub fn pause(&self) {
        self.player.pause();
    }

    pub fn seek(&self, position_ms: u32) {
        self.player.seek(position_ms);
    }

    pub fn set_volume(&self, volume: f32) {
        self.mixer
            .set_volume((volume.clamp(0., 1.) * f32::from(VolumeCtrl::MAX_VOLUME)) as u16);
    }

    pub fn stop(&self) {
        self.player.stop();
    }

    pub fn events(&self) -> PlayerEventChannel {
        self.player.get_player_event_channel()
    }

    pub fn is_connected(&self) -> bool {
        !self.session.is_invalid()
    }

    pub async fn radio_track_uris(&self, seed_uri: &str) -> Result<Vec<String>> {
        let seed = SpotifyUri::from_uri(seed_uri).context("invalid radio seed track URI")?;
        let response = self
            .session
            .spclient()
            .get_apollo_station("tracks", &seed.to_uri()?, Some(30), Vec::new(), true)
            .await
            .context("Spotify track radio endpoint failed")?;
        extract_track_uris(&response).context("Spotify track radio returned invalid JSON")
    }
}

fn extract_track_uris(json: &[u8]) -> serde_json::Result<Vec<String>> {
    fn visit(value: &serde_json::Value, seen: &mut HashSet<String>, uris: &mut Vec<String>) {
        match value {
            serde_json::Value::Array(values) => {
                for value in values {
                    visit(value, seen, uris);
                }
            }
            serde_json::Value::Object(values) => {
                for value in values.values() {
                    visit(value, seen, uris);
                }
            }
            serde_json::Value::String(value) => {
                let mut rest = value.as_str();
                while let Some(offset) = rest.find("spotify:track:") {
                    let candidate = &rest[offset..];
                    let id = candidate["spotify:track:".len()..]
                        .chars()
                        .take_while(char::is_ascii_alphanumeric)
                        .collect::<String>();
                    if id.len() == 22 {
                        let uri = format!("spotify:track:{id}");
                        if seen.insert(uri.clone()) {
                            uris.push(uri);
                        }
                    }
                    rest = &candidate["spotify:track:".len()..];
                }
            }
            _ => {}
        }
    }

    let value = serde_json::from_slice(json)?;
    let mut seen = HashSet::new();
    let mut uris = Vec::new();
    visit(&value, &mut seen, &mut uris);
    Ok(uris)
}

fn playback_token_entry() -> Result<Entry> {
    Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).map_err(Into::into)
}

fn playback_refresh_token() -> Result<Option<String>> {
    match playback_token_entry()?.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(anyhow!(error)),
    }
}

fn save_playback_refresh_token(refresh_token: &str) -> Result<()> {
    playback_token_entry()?
        .set_password(refresh_token)
        .map_err(Into::into)
}

pub fn delete_playback_refresh_token() -> Result<()> {
    match playback_token_entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::extract_track_uris;

    #[test]
    fn extracts_unique_track_uris_from_nested_radio_json() {
        let json = br#"{
            "items":[
                {"uri":"spotify:track:0123456789ABCDEFGHIJKL"},
                {"metadata":{"context":"before spotify:track:abcdefghijklmnopqrstuv after"}},
                {"uri":"spotify:track:0123456789ABCDEFGHIJKL"},
                {"uri":"spotify:album:0123456789ABCDEFGHIJKL"},
                {"uri":"spotify:track:short"}
            ]
        }"#;

        assert_eq!(
            extract_track_uris(json).unwrap(),
            vec![
                "spotify:track:0123456789ABCDEFGHIJKL",
                "spotify:track:abcdefghijklmnopqrstuv"
            ]
        );
    }
}
