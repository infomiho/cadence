use super::*;

impl CadenceApp {
    pub(super) fn handle_backend_events(
        &mut self,
        events: Vec<BackendEvent>,
        cx: &mut Context<Self>,
    ) {
        if events.is_empty() {
            return;
        }
        let player = self.player.clone();
        let session = self.session.clone();
        for event in events {
            let Some(event) =
                player.update(cx, |player, cx| player.handle_backend_event(event, cx))
            else {
                continue;
            };
            let Some(event) =
                session.update(cx, |session, cx| session.handle_backend_event(event, cx))
            else {
                continue;
            };
            match event {
                BackendEvent::SearchResults {
                    generation,
                    request_id,
                    tracks,
                    playlists,
                } => {
                    if is_current_response(
                        self.session.read(cx).generation(),
                        self.search_request_id,
                        generation,
                        request_id,
                    ) {
                        self.search_results = tracks.into();
                        self.search_playlists = playlists.into();
                        self.search_loaded = true;
                        self.searching = false;
                        self.search_error = None;
                    }
                }
                BackendEvent::SearchFailed {
                    generation,
                    request_id,
                    error,
                } => {
                    if is_current_response(
                        self.session.read(cx).generation(),
                        self.search_request_id,
                        generation,
                        request_id,
                    ) {
                        self.search_loaded = true;
                        self.searching = false;
                        self.search_error = Some(error.clone());
                        self.last_error = Some(error);
                    }
                }
                BackendEvent::LibraryLoaded {
                    generation,
                    liked_tracks,
                    playlists,
                } => {
                    if generation == self.session.read(cx).generation() {
                        self.liked_tracks = liked_tracks.into();
                        self.spotify_playlists = playlists.into();
                        self.library_loaded = true;
                        self.last_error = None;
                    }
                }
                BackendEvent::CachedLikedTracks { generation, tracks } => {
                    if generation == self.session.read(cx).generation() {
                        self.liked_tracks = tracks.into();
                    }
                }
                BackendEvent::LocalStateLoaded {
                    favorites,
                    pinned_playlists,
                    recently_played,
                } => {
                    self.favorite_keys = index_favorites(&favorites);
                    self.local_favorites = favorites.into();
                    self.pinned_playlists = pinned_playlists.into();
                    self.recently_played = recently_played.into();
                    self.local_state_loaded = true;
                    self.last_error = None;
                }
                BackendEvent::PlaylistLoaded {
                    generation,
                    request_id,
                    playlist,
                    tracks,
                } => {
                    if is_current_response(
                        self.session.read(cx).generation(),
                        self.playlist_request_id,
                        generation,
                        request_id,
                    ) && self
                        .selected_spotify_playlist
                        .as_ref()
                        .is_some_and(|selected| {
                            selected.provider == playlist.provider
                                && selected.source_id == playlist.source_id
                        })
                    {
                        self.playlist_tracks = tracks.into();
                        self.playlist_loaded = true;
                        self.playlist_error = None;
                        self.last_error = None;
                    }
                }
                BackendEvent::PlaylistFailed {
                    generation,
                    request_id,
                    source_id,
                    error,
                } => {
                    if is_current_response(
                        self.session.read(cx).generation(),
                        self.playlist_request_id,
                        generation,
                        request_id,
                    ) && self
                        .selected_spotify_playlist
                        .as_ref()
                        .is_some_and(|playlist| playlist.source_id == source_id)
                    {
                        self.playlist_loaded = true;
                        self.playlist_error = Some(error.clone());
                        self.last_error = Some(error);
                    }
                }
                BackendEvent::ArtistLoaded {
                    generation,
                    request_id,
                    source_id,
                    artist,
                    tracks,
                    albums,
                } => {
                    if is_current_response(
                        self.session.read(cx).generation(),
                        self.artist_request_id,
                        generation,
                        request_id,
                    ) && self
                        .selected_artist_ref
                        .as_ref()
                        .and_then(|artist| artist.source_id.as_deref())
                        == Some(source_id.as_str())
                    {
                        self.selected_artist = Some(artist);
                        self.artist_tracks = tracks.into();
                        self.artist_albums = albums.into();
                        self.artist_loaded = true;
                        self.artist_loading = false;
                        self.artist_error = None;
                        self.artist_loaded_at = Some(Instant::now());
                        self.last_error = None;
                    }
                }
                BackendEvent::ArtistFailed {
                    generation,
                    request_id,
                    source_id,
                    error,
                } => {
                    if is_current_response(
                        self.session.read(cx).generation(),
                        self.artist_request_id,
                        generation,
                        request_id,
                    ) && self
                        .selected_artist_ref
                        .as_ref()
                        .and_then(|artist| artist.source_id.as_deref())
                        == Some(source_id.as_str())
                    {
                        self.artist_loading = false;
                        if !self.artist_loaded {
                            self.artist_loaded = true;
                            self.artist_error = Some(error.clone());
                        }
                        self.last_error = Some(error);
                    }
                }
                BackendEvent::AlbumLoaded {
                    generation,
                    request_id,
                    source_id,
                    album,
                    tracks,
                } => {
                    if is_current_response(
                        self.session.read(cx).generation(),
                        self.album_request_id,
                        generation,
                        request_id,
                    ) && self
                        .selected_album_ref
                        .as_ref()
                        .and_then(|album| album.source_id.as_deref())
                        == Some(source_id.as_str())
                    {
                        self.selected_album = Some(album);
                        self.album_tracks = tracks.into();
                        self.album_loaded = true;
                        self.album_loading = false;
                        self.album_error = None;
                        self.album_loaded_at = Some(Instant::now());
                        self.last_error = None;
                    }
                }
                BackendEvent::AlbumFailed {
                    generation,
                    request_id,
                    source_id,
                    error,
                } => {
                    if is_current_response(
                        self.session.read(cx).generation(),
                        self.album_request_id,
                        generation,
                        request_id,
                    ) && self
                        .selected_album_ref
                        .as_ref()
                        .and_then(|album| album.source_id.as_deref())
                        == Some(source_id.as_str())
                    {
                        self.album_loading = false;
                        if !self.album_loaded {
                            self.album_loaded = true;
                            self.album_error = Some(error.clone());
                        }
                        self.last_error = Some(error);
                    }
                }
                BackendEvent::CatalogFailed { generation, error } => {
                    if generation == self.session.read(cx).generation() {
                        self.library_loaded = true;
                        self.last_error = Some(error);
                    }
                }
                BackendEvent::TrackFailed { error, .. } => {
                    self.last_error = Some(error);
                }
                BackendEvent::RadioFailed { request_id, error } => {
                    if self.pending_radio_request == Some(request_id) {
                        self.pending_radio_request = None;
                        self.player
                            .update(cx, |player, cx| player.set_loading(false, cx));
                        self.action_notice = Some(format!("Track radio unavailable: {error}"));
                    }
                }
                BackendEvent::RadioStarted { request_id } => {
                    if self.pending_radio_request == Some(request_id) {
                        self.pending_radio_request = None;
                        self.action_notice = None;
                    }
                }
                BackendEvent::RadioCancelled { request_id } => {
                    if self.pending_radio_request == Some(request_id) {
                        self.pending_radio_request = None;
                        self.player
                            .update(cx, |player, cx| player.set_loading(false, cx));
                        self.action_notice = None;
                    }
                }
                BackendEvent::Error(error) => {
                    self.last_error = Some(error);
                }
                // Consumed by the player and session entities before this match.
                BackendEvent::SetupRequired
                | BackendEvent::SpotifyConfigured { .. }
                | BackendEvent::SpotifyConfigurationFailed { .. }
                | BackendEvent::SpotifyConfigurationResetFailed(_)
                | BackendEvent::AuthorizationRequired
                | BackendEvent::LoggedOut
                | BackendEvent::CatalogReady { .. }
                | BackendEvent::ProfileLoaded { .. }
                | BackendEvent::AuthorizationFailed(_)
                | BackendEvent::FatalError(_)
                | BackendEvent::PlaybackReady
                | BackendEvent::PlaybackReconnecting
                | BackendEvent::PlaybackReconnected
                | BackendEvent::PlaybackRestored { .. }
                | BackendEvent::PlaybackSettled
                | BackendEvent::QueueEnded
                | BackendEvent::Playing { .. }
                | BackendEvent::Loading { .. }
                | BackendEvent::Paused { .. }
                | BackendEvent::EndOfTrack { .. }
                | BackendEvent::PositionChanged { .. }
                | BackendEvent::PlaybackSnapshotLoaded { .. }
                | BackendEvent::PlaybackContext { .. }
                | BackendEvent::PlaybackFailed(_) => {}
            }
        }
        cx.notify();
    }
}
