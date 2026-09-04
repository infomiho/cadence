use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Spotify,
    Tidal,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Spotify => "spotify",
            Self::Tidal => "tidal",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "spotify" => Some(Self::Spotify),
            "tidal" => Some(Self::Tidal),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArtistRef {
    pub name: String,
    pub source_id: Option<String>,
    pub spotify_uri: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlbumRef {
    pub name: String,
    pub source_id: Option<String>,
    pub spotify_uri: Option<String>,
    pub artwork_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Track {
    pub provider: Provider,
    pub source_id: String,
    pub spotify_uri: Option<String>,
    pub isrc: Option<String>,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub artists: Vec<ArtistRef>,
    pub album: String,
    #[serde(default)]
    pub album_ref: Option<AlbumRef>,
    pub duration_ms: u32,
    pub artwork_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Artist {
    pub provider: Provider,
    pub source_id: String,
    pub spotify_uri: Option<String>,
    pub name: String,
    pub artwork_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Album {
    pub provider: Provider,
    pub source_id: String,
    pub spotify_uri: Option<String>,
    pub name: String,
    pub artists: Vec<ArtistRef>,
    pub release_date: Option<String>,
    pub artwork_url: Option<String>,
    pub track_count: Option<u32>,
}

impl Track {
    pub fn is_displayable(&self) -> bool {
        !self.title.trim().is_empty() && !self.artist.trim().is_empty() && self.duration_ms > 0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Playlist {
    pub provider: Provider,
    pub source_id: String,
    pub name: String,
    pub owner: String,
    pub track_count: u32,
    pub artwork_url: Option<String>,
}

/// What a cheap reload compares before committing to a full walk: the first
/// pages and Spotify's totals for both collections. The totals catch
/// removals past the first page; the heads catch additions and renames; the
/// snapshot ids catch edits inside a playlist, which move neither list.
/// Persisted beside the cache so the first probe after launch can answer too.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LibraryFingerprint {
    pub liked_head: Vec<String>,
    pub liked_total: u32,
    pub playlist_head: Vec<(String, String, String)>,
    pub playlist_total: u32,
}

/// The library as it was last persisted, painted before Spotify answers.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CachedLibrary {
    pub liked_tracks: Vec<Track>,
    pub playlists: Vec<Playlist>,
    pub fingerprint: Option<LibraryFingerprint>,
}

impl CachedLibrary {
    pub fn is_empty(&self) -> bool {
        self.liked_tracks.is_empty() && self.playlists.is_empty()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UserProfile {
    pub display_name: String,
    pub artwork_url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueueItem {
    pub id: i64,
    pub track: Track,
}

#[cfg(test)]
mod tests {
    use super::Track;

    #[test]
    fn persisted_tracks_without_artist_references_still_load() {
        let track: Track = serde_json::from_str(
            r#"{
                "provider":"spotify",
                "source_id":"track",
                "spotify_uri":"spotify:track:track",
                "isrc":null,
                "title":"Title",
                "artist":"Artist",
                "album":"Album",
                "duration_ms":1000,
                "artwork_url":null
            }"#,
        )
        .unwrap();

        assert!(track.artists.is_empty());
        assert!(track.album_ref.is_none());
    }
}
