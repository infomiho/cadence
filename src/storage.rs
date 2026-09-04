use std::{path::Path, time::Duration};

use anyhow::{Context as _, Result};
use directories::ProjectDirs;
use rusqlite::{Connection, params};

use crate::model::{CachedLibrary, LibraryFingerprint, Playlist, QueueItem, Track};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MascotPreference {
    #[default]
    None,
    RomeoVespa,
    VespaDuo,
    TarantellaDancer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppPreferences {
    pub sidebar_collapsed: bool,
    pub theme: ThemePreference,
    pub autoplay: bool,
    pub mascot: MascotPreference,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            sidebar_collapsed: false,
            theme: ThemePreference::default(),
            // Autoplay is on unless the listener turned it off.
            autoplay: true,
            mascot: MascotPreference::default(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackSnapshot {
    pub tracks: Vec<Track>,
    pub index: usize,
    pub position_ms: u32,
}

pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn open_default() -> Result<Self> {
        let project = ProjectDirs::from("com", "Cadence", "Cadence")
            .context("could not resolve the Cadence data directory")?;
        std::fs::create_dir_all(project.data_dir())
            .context("could not create the Cadence data directory")?;
        Self::open(project.data_dir().join("cadence.sqlite3"))
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let connection = Connection::open(path).context("could not open the Cadence database")?;
        connection.busy_timeout(Duration::from_secs(2))?;
        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    #[cfg(test)]
    pub(crate) fn in_memory() -> Result<Self> {
        let connection = Connection::open_in_memory()?;
        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        self.connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;",
        )?;
        let version: u32 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > 5 {
            anyhow::bail!("database schema version {version} is newer than this Cadence build");
        }
        if version < 4 {
            self.migrate_to_4(version)?;
        }
        if version < 5 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE playlists_cache (
                     position INTEGER PRIMARY KEY,
                     playlist_json TEXT NOT NULL
                 );
                 PRAGMA user_version = 5;
                 COMMIT;",
            )?;
        }
        Ok(())
    }

    fn migrate_to_4(&self, version: u32) -> Result<()> {
        if version == 0 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;

             CREATE TABLE IF NOT EXISTS favorites (
                 provider TEXT NOT NULL,
                 source_id TEXT NOT NULL,
                 track_json TEXT NOT NULL,
                 created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                 PRIMARY KEY (provider, source_id)
             );

             CREATE TABLE IF NOT EXISTS pinned_playlists (
                 provider TEXT NOT NULL,
                 source_id TEXT NOT NULL,
                 playlist_json TEXT NOT NULL,
                 created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                 PRIMARY KEY (provider, source_id)
             );

             CREATE TABLE IF NOT EXISTS history (
                 id INTEGER PRIMARY KEY,
                 provider TEXT NOT NULL,
                 source_id TEXT NOT NULL,
                 track_json TEXT NOT NULL,
                 played_at INTEGER NOT NULL DEFAULT (unixepoch())
             );

             CREATE TABLE IF NOT EXISTS queue (
                 position INTEGER PRIMARY KEY,
                 item_id INTEGER NOT NULL,
                 track_json TEXT NOT NULL
             );

             CREATE TABLE IF NOT EXISTS liked_tracks_cache (
                 position INTEGER PRIMARY KEY,
                 track_json TEXT NOT NULL,
                 refreshed_at INTEGER NOT NULL
             );

             CREATE TABLE IF NOT EXISTS playback_state (
                 id INTEGER PRIMARY KEY CHECK (id = 1),
                 tracks_json TEXT NOT NULL,
                 current_index INTEGER NOT NULL,
                 position_ms INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL DEFAULT (unixepoch())
             );

             CREATE TABLE IF NOT EXISTS preferences (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );

             PRAGMA user_version = 4;
             COMMIT;",
            )?;
        } else if version == 1 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE liked_tracks_cache (
                     position INTEGER PRIMARY KEY,
                     track_json TEXT NOT NULL,
                      refreshed_at INTEGER NOT NULL
                  );
                   CREATE TABLE playback_state (
                      id INTEGER PRIMARY KEY CHECK (id = 1),
                      tracks_json TEXT NOT NULL,
                      current_index INTEGER NOT NULL,
                      position_ms INTEGER NOT NULL,
                      updated_at INTEGER NOT NULL DEFAULT (unixepoch())
                   );
                   CREATE TABLE preferences (
                       key TEXT PRIMARY KEY,
                       value TEXT NOT NULL
                   );
                   PRAGMA user_version = 4;
                  COMMIT;",
            )?;
        } else if version == 2 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                  CREATE TABLE playback_state (
                     id INTEGER PRIMARY KEY CHECK (id = 1),
                     tracks_json TEXT NOT NULL,
                     current_index INTEGER NOT NULL,
                     position_ms INTEGER NOT NULL,
                     updated_at INTEGER NOT NULL DEFAULT (unixepoch())
                  );
                  CREATE TABLE preferences (
                      key TEXT PRIMARY KEY,
                      value TEXT NOT NULL
                  );
                  PRAGMA user_version = 4;
                  COMMIT;",
            )?;
        } else {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE preferences (
                     key TEXT PRIMARY KEY,
                     value TEXT NOT NULL
                 );
                 PRAGMA user_version = 4;
                 COMMIT;",
            )?;
        }
        Ok(())
    }

    pub fn preferences(&self) -> Result<AppPreferences> {
        let sidebar_collapsed = self
            .preference("sidebar_collapsed")?
            .is_some_and(|value| value == "true");
        let theme = match self.preference("theme")?.as_deref() {
            Some("light") => ThemePreference::Light,
            Some("dark") => ThemePreference::Dark,
            _ => ThemePreference::System,
        };
        let autoplay = self.preference("autoplay")?.as_deref() != Some("false");
        let mascot = match self.preference("mascot")?.as_deref() {
            Some("none") => MascotPreference::None,
            Some("vespa_duo") => MascotPreference::VespaDuo,
            Some("tarantella_dancer") => MascotPreference::TarantellaDancer,
            Some("romeo_vespa") => MascotPreference::RomeoVespa,
            _ => MascotPreference::default(),
        };
        Ok(AppPreferences {
            sidebar_collapsed,
            theme,
            autoplay,
            mascot,
        })
    }

    pub fn set_autoplay(&mut self, autoplay: bool) -> Result<()> {
        self.set_preference("autoplay", if autoplay { "true" } else { "false" })
    }

    pub fn set_mascot(&mut self, mascot: MascotPreference) -> Result<()> {
        let value = match mascot {
            MascotPreference::None => "none",
            MascotPreference::RomeoVespa => "romeo_vespa",
            MascotPreference::VespaDuo => "vespa_duo",
            MascotPreference::TarantellaDancer => "tarantella_dancer",
        };
        self.set_preference("mascot", value)
    }

    pub fn set_sidebar_collapsed(&mut self, collapsed: bool) -> Result<()> {
        self.set_preference(
            "sidebar_collapsed",
            if collapsed { "true" } else { "false" },
        )
    }

    pub fn set_theme_preference(&mut self, theme: ThemePreference) -> Result<()> {
        let value = match theme {
            ThemePreference::System => "system",
            ThemePreference::Light => "light",
            ThemePreference::Dark => "dark",
        };
        self.set_preference("theme", value)
    }

    pub fn spotify_client_id(&self) -> Result<Option<String>> {
        Ok(self
            .preference("spotify_client_id")?
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()))
    }

    pub fn set_spotify_client_id(&mut self, client_id: &str) -> Result<()> {
        self.set_preference("spotify_client_id", client_id.trim())
    }

    pub fn configure_spotify(&mut self, client_id: &str) -> Result<()> {
        let transaction = self.connection.transaction()?;
        for (key, value) in [
            ("spotify_client_id", client_id.trim()),
            ("spotify_oauth_credentials_invalidated", "true"),
            ("spotify_playback_credentials_invalidated", "true"),
        ] {
            transaction.execute(
                "INSERT INTO preferences (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn remove_spotify_client_id(&mut self) -> Result<()> {
        self.connection.execute(
            "DELETE FROM preferences WHERE key = ?1",
            params!["spotify_client_id"],
        )?;
        Ok(())
    }

    pub fn reset_spotify_configuration(&mut self) -> Result<()> {
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "DELETE FROM preferences WHERE key = ?1",
            params!["spotify_client_id"],
        )?;
        for key in [
            "spotify_oauth_credentials_invalidated",
            "spotify_playback_credentials_invalidated",
        ] {
            transaction.execute(
                "INSERT INTO preferences (key, value) VALUES (?1, 'true')
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn spotify_oauth_credentials_invalidated(&self) -> Result<bool> {
        Ok(self
            .preference("spotify_oauth_credentials_invalidated")?
            .is_some_and(|value| value == "true"))
    }

    pub fn set_spotify_oauth_credentials_invalidated(&mut self, invalidated: bool) -> Result<()> {
        self.set_preference(
            "spotify_oauth_credentials_invalidated",
            if invalidated { "true" } else { "false" },
        )
    }

    pub fn spotify_playback_credentials_invalidated(&self) -> Result<bool> {
        Ok(self
            .preference("spotify_playback_credentials_invalidated")?
            .is_some_and(|value| value == "true"))
    }

    pub fn set_spotify_playback_credentials_invalidated(
        &mut self,
        invalidated: bool,
    ) -> Result<()> {
        self.set_preference(
            "spotify_playback_credentials_invalidated",
            if invalidated { "true" } else { "false" },
        )
    }

    fn preference(&self, key: &str) -> Result<Option<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT value FROM preferences WHERE key = ?1")?;
        let mut rows = statement.query(params![key])?;
        Ok(rows.next()?.map(|row| row.get(0)).transpose()?)
    }

    fn set_preference(&mut self, key: &str, value: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO preferences (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn set_favorite(&mut self, track: &Track, favorite: bool) -> Result<()> {
        if favorite {
            self.connection.execute(
                "INSERT INTO favorites (provider, source_id, track_json)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT (provider, source_id) DO UPDATE SET track_json = excluded.track_json",
                params![
                    track.provider.as_str(),
                    track.source_id,
                    serde_json::to_string(track)?
                ],
            )?;
        } else {
            self.connection.execute(
                "DELETE FROM favorites WHERE provider = ?1 AND source_id = ?2",
                params![track.provider.as_str(), track.source_id],
            )?;
        }
        Ok(())
    }

    pub fn favorites(&self) -> Result<Vec<Track>> {
        let mut statement = self
            .connection
            .prepare("SELECT track_json FROM favorites ORDER BY created_at DESC")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }

    pub fn set_playlist_pinned(&mut self, playlist: &Playlist, pinned: bool) -> Result<()> {
        if pinned {
            self.connection.execute(
                "INSERT INTO pinned_playlists (provider, source_id, playlist_json)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT (provider, source_id) DO UPDATE SET playlist_json = excluded.playlist_json",
                params![
                    playlist.provider.as_str(),
                    playlist.source_id,
                    serde_json::to_string(playlist)?
                ],
            )?;
        } else {
            self.connection.execute(
                "DELETE FROM pinned_playlists WHERE provider = ?1 AND source_id = ?2",
                params![playlist.provider.as_str(), playlist.source_id],
            )?;
        }
        Ok(())
    }

    pub fn pinned_playlists(&self) -> Result<Vec<Playlist>> {
        let mut statement = self
            .connection
            .prepare("SELECT playlist_json FROM pinned_playlists ORDER BY created_at DESC")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }

    pub fn add_history(&mut self, track: &Track) -> Result<()> {
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO history (provider, source_id, track_json) VALUES (?1, ?2, ?3)",
            params![
                track.provider.as_str(),
                track.source_id,
                serde_json::to_string(track)?
            ],
        )?;
        transaction.execute(
            "DELETE FROM history
             WHERE provider = ?1 AND source_id = ?2 AND id <> last_insert_rowid()",
            params![track.provider.as_str(), track.source_id],
        )?;
        transaction.execute(
            "DELETE FROM history
             WHERE id NOT IN (SELECT id FROM history ORDER BY played_at DESC, id DESC LIMIT 500)",
            [],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn recent_tracks(&self, limit: usize) -> Result<Vec<Track>> {
        let mut statement = self.connection.prepare(
            "SELECT track_json
             FROM (
                 SELECT track_json, played_at, id,
                        ROW_NUMBER() OVER (
                            PARTITION BY provider, source_id
                            ORDER BY played_at DESC, id DESC
                        ) AS recency
                 FROM history
             )
             WHERE recency = 1
             ORDER BY played_at DESC, id DESC
             LIMIT ?1",
        )?;
        let limit = i64::try_from(limit).context("history limit exceeds SQLite range")?;
        let rows = statement.query_map([limit], |row| row.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }

    pub fn replace_queue(&mut self, queue: &[QueueItem]) -> Result<()> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM queue", [])?;
        for (position, item) in queue.iter().enumerate() {
            let position =
                i64::try_from(position).context("queue position exceeds SQLite range")?;
            transaction.execute(
                "INSERT INTO queue (position, item_id, track_json) VALUES (?1, ?2, ?3)",
                params![position, item.id, serde_json::to_string(&item.track)?],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn queue(&self) -> Result<Vec<QueueItem>> {
        let mut statement = self
            .connection
            .prepare("SELECT item_id, track_json FROM queue ORDER BY position")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (id, json) = row?;
            Ok(QueueItem {
                id,
                track: serde_json::from_str(&json)?,
            })
        })
        .collect()
    }

    pub fn set_playback_state(
        &self,
        tracks: &[Track],
        index: usize,
        position_ms: u32,
    ) -> Result<()> {
        anyhow::ensure!(
            tracks.get(index).is_some(),
            "playback index is out of bounds"
        );
        let index = i64::try_from(index).context("playback index exceeds SQLite range")?;
        self.connection.execute(
            "INSERT INTO playback_state (id, tracks_json, current_index, position_ms, updated_at)
             VALUES (1, ?1, ?2, ?3, unixepoch())
             ON CONFLICT (id) DO UPDATE SET
                 tracks_json = excluded.tracks_json,
                 current_index = excluded.current_index,
                 position_ms = excluded.position_ms,
                 updated_at = excluded.updated_at",
            params![
                serde_json::to_string(tracks)?,
                index,
                i64::from(position_ms)
            ],
        )?;
        Ok(())
    }

    pub fn update_playback_position(&self, position_ms: u32) -> Result<()> {
        self.connection.execute(
            "UPDATE playback_state SET position_ms = ?1, updated_at = unixepoch() WHERE id = 1",
            [i64::from(position_ms)],
        )?;
        Ok(())
    }

    pub fn playback_state(&self) -> Result<Option<PlaybackSnapshot>> {
        let state = self.connection.query_row(
            "SELECT tracks_json, current_index, position_ms FROM playback_state WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        );
        let (tracks_json, index, position_ms) = match state {
            Ok(state) => state,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let tracks: Vec<Track> = serde_json::from_str(&tracks_json)?;
        let index = usize::try_from(index).context("stored playback index is invalid")?;
        anyhow::ensure!(
            tracks.get(index).is_some(),
            "stored playback index is out of bounds"
        );
        Ok(Some(PlaybackSnapshot {
            tracks,
            index,
            position_ms: u32::try_from(position_ms)
                .context("stored playback position is invalid")?,
        }))
    }

    pub fn clear_playback_state(&self) -> Result<()> {
        self.connection.execute("DELETE FROM playback_state", [])?;
        Ok(())
    }

    /// Replaces the cached library in one transaction, so the fingerprint can
    /// never describe contents other than the ones on disk.
    pub fn replace_library_cache(
        &mut self,
        tracks: &[Track],
        playlists: &[Playlist],
        fingerprint: Option<&LibraryFingerprint>,
    ) -> Result<()> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM liked_tracks_cache", [])?;
        for (position, track) in tracks
            .iter()
            .filter(|track| track.is_displayable())
            .enumerate()
        {
            let position =
                i64::try_from(position).context("liked-track position exceeds SQLite range")?;
            transaction.execute(
                "INSERT INTO liked_tracks_cache (position, track_json, refreshed_at)
                 VALUES (?1, ?2, unixepoch())",
                params![position, serde_json::to_string(track)?],
            )?;
        }
        transaction.execute("DELETE FROM playlists_cache", [])?;
        for (position, playlist) in playlists.iter().enumerate() {
            let position =
                i64::try_from(position).context("playlist position exceeds SQLite range")?;
            transaction.execute(
                "INSERT INTO playlists_cache (position, playlist_json) VALUES (?1, ?2)",
                params![position, serde_json::to_string(playlist)?],
            )?;
        }
        match fingerprint {
            Some(fingerprint) => transaction.execute(
                "INSERT INTO preferences (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![LIBRARY_FINGERPRINT_KEY, serde_json::to_string(fingerprint)?],
            )?,
            None => transaction.execute(
                "DELETE FROM preferences WHERE key = ?1",
                params![LIBRARY_FINGERPRINT_KEY],
            )?,
        };
        transaction.commit()?;
        Ok(())
    }

    pub fn clear_library_cache(&mut self) -> Result<()> {
        self.replace_library_cache(&[], &[], None)
    }

    /// A fingerprint this build cannot read is dropped rather than failing the
    /// whole read: the contents still paint, and the next probe walks in full.
    pub fn cached_library(&self) -> Result<CachedLibrary> {
        let fingerprint = self
            .preference(LIBRARY_FINGERPRINT_KEY)?
            .and_then(|json| serde_json::from_str(&json).ok());
        Ok(CachedLibrary {
            liked_tracks: self.liked_tracks()?,
            playlists: self.cached_playlists()?,
            fingerprint,
        })
    }

    pub fn liked_tracks(&self) -> Result<Vec<Track>> {
        let mut statement = self
            .connection
            .prepare("SELECT track_json FROM liked_tracks_cache ORDER BY position")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?))
            .filter(|track: &Result<Track>| track.as_ref().is_ok_and(Track::is_displayable))
            .collect()
    }

    fn cached_playlists(&self) -> Result<Vec<Playlist>> {
        let mut statement = self
            .connection
            .prepare("SELECT playlist_json FROM playlists_cache ORDER BY position")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }
}

const LIBRARY_FINGERPRINT_KEY: &str = "library_fingerprint";

#[cfg(test)]
mod tests {
    use super::{AppPreferences, MascotPreference, Store, ThemePreference};
    use crate::model::{LibraryFingerprint, Playlist, Provider, QueueItem, Track};

    fn track(id: &str) -> Track {
        Track {
            provider: Provider::Spotify,
            source_id: id.to_owned(),
            spotify_uri: Some(format!("spotify:track:{id}")),
            isrc: None,
            title: format!("Track {id}"),
            artist: "Artist".to_owned(),
            artists: Vec::new(),
            album: "Album".to_owned(),
            album_ref: None,
            duration_ms: 180_000,
            artwork_url: None,
        }
    }

    #[test]
    fn favorites_can_be_added_and_removed() {
        let mut store = Store::in_memory().unwrap();
        let track = track("one");

        store.set_favorite(&track, true).unwrap();
        assert_eq!(store.favorites().unwrap(), vec![track.clone()]);

        store.set_favorite(&track, false).unwrap();
        assert!(store.favorites().unwrap().is_empty());
    }

    #[test]
    fn preferences_round_trip() {
        let mut store = Store::in_memory().unwrap();
        assert_eq!(store.preferences().unwrap(), AppPreferences::default());

        store.set_sidebar_collapsed(true).unwrap();
        store.set_theme_preference(ThemePreference::Dark).unwrap();
        store.set_autoplay(false).unwrap();
        store
            .set_mascot(MascotPreference::TarantellaDancer)
            .unwrap();
        assert_eq!(
            store.preferences().unwrap(),
            AppPreferences {
                sidebar_collapsed: true,
                theme: ThemePreference::Dark,
                autoplay: false,
                mascot: MascotPreference::TarantellaDancer,
            }
        );
    }

    #[test]
    fn mascot_preferences_round_trip() {
        let mut store = Store::in_memory().unwrap();
        assert_eq!(store.preferences().unwrap().mascot, MascotPreference::None);

        for mascot in [
            MascotPreference::None,
            MascotPreference::RomeoVespa,
            MascotPreference::VespaDuo,
            MascotPreference::TarantellaDancer,
        ] {
            store.set_mascot(mascot).unwrap();
            assert_eq!(store.preferences().unwrap().mascot, mascot);
        }
    }

    #[test]
    fn invalid_preferences_use_safe_defaults() {
        let mut store = Store::in_memory().unwrap();
        store
            .set_preference("sidebar_collapsed", "sometimes")
            .unwrap();
        store.set_preference("theme", "midnight").unwrap();

        assert_eq!(store.preferences().unwrap(), AppPreferences::default());
    }

    #[test]
    fn spotify_client_id_round_trips_and_can_be_removed() {
        let mut store = Store::in_memory().unwrap();

        assert_eq!(store.spotify_client_id().unwrap(), None);
        store
            .set_spotify_client_id(" 0123456789abcdef0123456789abcdef ")
            .unwrap();
        assert_eq!(
            store.spotify_client_id().unwrap().as_deref(),
            Some("0123456789abcdef0123456789abcdef")
        );

        store.remove_spotify_client_id().unwrap();
        assert_eq!(store.spotify_client_id().unwrap(), None);
    }

    #[test]
    fn blank_spotify_client_id_is_not_configured() {
        let mut store = Store::in_memory().unwrap();

        store.set_spotify_client_id("   ").unwrap();

        assert_eq!(store.spotify_client_id().unwrap(), None);
    }

    #[test]
    fn spotify_configuration_and_credential_invalidation_round_trip() {
        let mut store = Store::in_memory().unwrap();

        store
            .configure_spotify("0123456789abcdef0123456789abcdef")
            .unwrap();
        assert_eq!(
            store.spotify_client_id().unwrap().as_deref(),
            Some("0123456789abcdef0123456789abcdef")
        );
        assert!(store.spotify_oauth_credentials_invalidated().unwrap());
        assert!(store.spotify_playback_credentials_invalidated().unwrap());

        store
            .set_spotify_oauth_credentials_invalidated(false)
            .unwrap();
        store
            .set_spotify_playback_credentials_invalidated(false)
            .unwrap();
        assert!(!store.spotify_oauth_credentials_invalidated().unwrap());
        assert!(!store.spotify_playback_credentials_invalidated().unwrap());

        store.reset_spotify_configuration().unwrap();
        assert_eq!(store.spotify_client_id().unwrap(), None);
        assert!(store.spotify_oauth_credentials_invalidated().unwrap());
        assert!(store.spotify_playback_credentials_invalidated().unwrap());
    }

    #[test]
    fn queue_order_and_duplicates_are_persisted() {
        let mut store = Store::in_memory().unwrap();
        let queue = vec![
            QueueItem {
                id: 10,
                track: track("one"),
            },
            QueueItem {
                id: 11,
                track: track("two"),
            },
            QueueItem {
                id: 12,
                track: track("one"),
            },
        ];

        store.replace_queue(&queue).unwrap();
        assert_eq!(store.queue().unwrap(), queue);
    }

    #[test]
    fn playback_state_round_trips_and_clears() {
        let store = Store::in_memory().unwrap();
        let tracks = vec![track("one"), track("two")];

        store.set_playback_state(&tracks, 1, 42_000).unwrap();
        let state = store.playback_state().unwrap().unwrap();
        assert_eq!(state.tracks, tracks);
        assert_eq!(state.index, 1);
        assert_eq!(state.position_ms, 42_000);

        store.update_playback_position(45_000).unwrap();
        assert_eq!(store.playback_state().unwrap().unwrap().position_ms, 45_000);

        store.clear_playback_state().unwrap();
        assert!(store.playback_state().unwrap().is_none());
    }

    #[test]
    fn liked_track_cache_replaces_the_previous_snapshot() {
        let mut store = Store::in_memory().unwrap();
        store
            .replace_library_cache(&[track("one"), track("two")], &[], None)
            .unwrap();
        assert_eq!(
            store
                .liked_tracks()
                .unwrap()
                .into_iter()
                .map(|track| track.source_id)
                .collect::<Vec<_>>(),
            ["one", "two"]
        );

        store
            .replace_library_cache(&[track("three")], &[], None)
            .unwrap();
        assert_eq!(store.liked_tracks().unwrap(), vec![track("three")]);
    }

    #[test]
    fn library_cache_keeps_playlists_and_fingerprint_beside_liked_tracks() {
        let mut store = Store::in_memory().unwrap();
        let playlist = Playlist {
            provider: Provider::Spotify,
            source_id: "list".to_owned(),
            name: "Road".to_owned(),
            owner: "me".to_owned(),
            track_count: 3,
            artwork_url: None,
        };
        let fingerprint = LibraryFingerprint {
            liked_head: vec!["one".to_owned()],
            liked_total: 1,
            playlist_head: vec![("list".to_owned(), "Road".to_owned(), "snap".to_owned())],
            playlist_total: 1,
        };

        store
            .replace_library_cache(
                &[track("one")],
                std::slice::from_ref(&playlist),
                Some(&fingerprint),
            )
            .unwrap();
        let cached = store.cached_library().unwrap();
        assert_eq!(cached.liked_tracks, vec![track("one")]);
        assert_eq!(cached.playlists, vec![playlist]);
        assert_eq!(cached.fingerprint, Some(fingerprint));

        store.clear_library_cache().unwrap();
        let cleared = store.cached_library().unwrap();
        assert!(cleared.is_empty());
        assert_eq!(cleared.fingerprint, None);
    }

    #[test]
    fn liked_track_cache_ignores_incomplete_tracks() {
        let mut store = Store::in_memory().unwrap();
        let mut incomplete = track("missing");
        incomplete.title.clear();
        incomplete.artist.clear();
        incomplete.duration_ms = 0;

        store
            .replace_library_cache(&[track("one"), incomplete.clone()], &[], None)
            .unwrap();
        assert_eq!(store.liked_tracks().unwrap(), vec![track("one")]);

        store
            .connection
            .execute(
                "INSERT INTO liked_tracks_cache (position, track_json, refreshed_at)
                 VALUES (1, ?1, unixepoch())",
                [serde_json::to_string(&incomplete).unwrap()],
            )
            .unwrap();
        assert_eq!(store.liked_tracks().unwrap(), vec![track("one")]);
    }

    #[test]
    fn recent_tracks_are_newest_first() {
        let mut store = Store::in_memory().unwrap();
        store.add_history(&track("one")).unwrap();
        store.add_history(&track("two")).unwrap();
        store.add_history(&track("one")).unwrap();

        let recent = store.recent_tracks(10).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].source_id, "one");
        assert_eq!(recent[1].source_id, "two");
    }

    #[test]
    fn playlists_can_be_pinned_and_unpinned() {
        let mut store = Store::in_memory().unwrap();
        let playlist = Playlist {
            provider: Provider::Spotify,
            source_id: "focus".to_owned(),
            name: "Focus".to_owned(),
            owner: "Owner".to_owned(),
            track_count: 10,
            artwork_url: None,
        };

        store.set_playlist_pinned(&playlist, true).unwrap();
        assert_eq!(store.pinned_playlists().unwrap(), vec![playlist.clone()]);

        store.set_playlist_pinned(&playlist, false).unwrap();
        assert!(store.pinned_playlists().unwrap().is_empty());
    }
}
