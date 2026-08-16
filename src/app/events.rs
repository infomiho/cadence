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
        for event in events {
            match event {
                BackendEvent::SetupRequired => {
                    self.connection_state = ConnectionState::SetupRequired;
                    self.spotify_client_id = None;
                    self.spotify_client_id_source = None;
                    self.spotify_setup_needs_focus = true;
                    self.spotify_configuration_blocked = false;
                }
                BackendEvent::SpotifyConfigured {
                    generation,
                    client_id,
                    source,
                } => {
                    self.spotify_client_id = Some(client_id);
                    self.spotify_client_id_source = Some(source);
                    self.spotify_configuration_blocked = false;
                    if self.pending_spotify_configuration == Some(generation) {
                        self.pending_spotify_configuration = None;
                        self.connection_state = ConnectionState::Connecting;
                        self.authenticate();
                    }
                }
                BackendEvent::SpotifyConfigurationFailed { generation, error } => {
                    if generation == 0
                        && self.spotify_client_id_source == Some(ClientIdSource::Environment)
                    {
                        self.connection_state = ConnectionState::AuthorizationRequired;
                        self.spotify_configuration_blocked = true;
                        self.last_error = Some(error);
                    } else if generation == 0
                        || self.pending_spotify_configuration == Some(generation)
                    {
                        self.pending_spotify_configuration = None;
                        self.connection_state = ConnectionState::SetupRequired;
                        self.spotify_setup_error = Some(format!(
                            "Could not configure Spotify. Check the Client ID and try again. {error}"
                        ));
                        self.spotify_setup_needs_focus = true;
                    }
                }
                BackendEvent::SpotifyConfigurationResetFailed(error) => {
                    self.connection_state = ConnectionState::Ready;
                    self.action_notice = Some(format!(
                        "Unable to restart Spotify setup. Check your connection and try again. {error}"
                    ));
                }
                BackendEvent::AuthorizationRequired => {
                    if !matches!(self.connection_state, ConnectionState::Connecting) {
                        self.connection_state = ConnectionState::AuthorizationRequired;
                    }
                }
                BackendEvent::LoggedOut => {
                    self.connection_state = ConnectionState::AuthorizationRequired;
                    self.route = Route::LikedSongs;
                    self.profile = None;
                    self.liked_tracks = Arc::default();
                    self.spotify_playlists = Arc::default();
                    self.search_results = Arc::default();
                    self.search_playlists = Arc::default();
                    self.search_loaded = false;
                    self.searching = false;
                    self.search_error = None;
                    next_request_id(&mut self.search_request_id);
                    self.library_loaded = false;
                    self.selected_spotify_playlist = None;
                    self.playlist_tracks = Arc::default();
                    self.playlist_loaded = false;
                    self.playlist_error = None;
                    self.selected_artist_ref = None;
                    self.selected_artist = None;
                    self.artist_tracks = Arc::default();
                    self.artist_albums = Arc::default();
                    self.artist_loaded = false;
                    self.artist_loading = false;
                    self.artist_error = None;
                    self.artist_loaded_at = None;
                    self.selected_album_ref = None;
                    self.selected_album = None;
                    self.album_tracks = Arc::default();
                    self.album_loaded = false;
                    self.album_loading = false;
                    self.album_error = None;
                    self.album_loaded_at = None;
                    self.now_playing = None;
                    self.playback_context = Arc::default();
                    self.queue = Arc::default();
                    self.playing = false;
                    self.playback_loading = false;
                    self.playback_restore = None;
                    self.position_ms = 0;
                    self.last_saved_position_ms = 0;
                    self.playback_error = None;
                    self.last_error = None;
                    self.action_notice = None;
                    self.pending_radio_request = None;
                }
                BackendEvent::CatalogReady { generation } => {
                    if generation == self.account_generation {
                        self.connection_state = ConnectionState::Ready;
                        self.last_error = None;
                    }
                }
                BackendEvent::PlaybackReady => {
                    self.playback_error = None;
                    self.send_backend(BackendCommand::SetVolume(self.volume));
                }
                BackendEvent::PlaybackReconnecting => {
                    if self.playback_restore.is_none() && self.now_playing.is_some() {
                        self.playback_restore = Some((self.position_ms, self.playing));
                    }
                    self.playback_loading = true;
                }
                BackendEvent::PlaybackReconnected => {
                    self.playback_error = None;
                    self.send_backend(BackendCommand::SetVolume(self.volume));
                    if let Some((position_ms, playing)) = self.playback_restore {
                        self.send_backend(BackendCommand::RestorePlayback {
                            position_ms,
                            playing,
                        });
                    } else {
                        self.playback_loading = false;
                    }
                }
                BackendEvent::PlaybackRestored {
                    position_ms,
                    playing,
                } => {
                    self.position_ms = position_ms;
                    self.last_saved_position_ms = position_ms;
                    self.playing = playing;
                    self.playback_loading = false;
                    self.playback_restore = None;
                }
                BackendEvent::PlaybackSettled => {
                    self.playback_loading = false;
                    self.playback_restore = None;
                }
                BackendEvent::QueueEnded => {
                    self.playing = false;
                    self.playback_loading = false;
                }
                BackendEvent::SearchResults {
                    generation,
                    request_id,
                    tracks,
                    playlists,
                } => {
                    if is_current_response(
                        self.account_generation,
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
                        self.account_generation,
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
                    if generation == self.account_generation {
                        self.liked_tracks = liked_tracks.into();
                        self.spotify_playlists = playlists.into();
                        self.library_loaded = true;
                        self.last_error = None;
                    }
                }
                BackendEvent::ProfileLoaded {
                    generation,
                    profile,
                } => {
                    if generation == self.account_generation {
                        self.profile = Some(profile);
                    }
                }
                BackendEvent::CachedLikedTracks { generation, tracks } => {
                    if generation == self.account_generation {
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
                        self.account_generation,
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
                        self.account_generation,
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
                        self.account_generation,
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
                        self.account_generation,
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
                        self.account_generation,
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
                        self.account_generation,
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
                BackendEvent::Playing { spotify_uri } => {
                    if self.playback_restore.is_none() && self.live_track_matches(&spotify_uri) {
                        self.playing = true;
                        self.playback_loading = false;
                    }
                }
                BackendEvent::Loading { spotify_uri } => {
                    if self.playback_restore.is_none() && self.live_track_matches(&spotify_uri) {
                        self.playback_loading = true;
                    }
                }
                BackendEvent::Paused { spotify_uri } => {
                    if self.playback_restore.is_none() && self.live_track_matches(&spotify_uri) {
                        self.playing = false;
                        self.playback_loading = false;
                        if self.position_ms != self.last_saved_position_ms {
                            self.send_backend(BackendCommand::SavePlaybackPosition {
                                spotify_uri,
                                position_ms: self.position_ms,
                            });
                            self.last_saved_position_ms = self.position_ms;
                        }
                    }
                }
                BackendEvent::EndOfTrack { spotify_uri } => {
                    if self.playback_restore.is_none() && self.live_track_matches(&spotify_uri) {
                        self.playing = false;
                        self.playback_loading = false;
                        self.send_backend(BackendCommand::Next);
                    }
                }
                BackendEvent::PositionChanged {
                    spotify_uri,
                    position_ms,
                } => {
                    if self.playback_restore.is_none() && self.live_track_matches(&spotify_uri) {
                        self.position_ms = position_ms;
                        if position_ms.abs_diff(self.last_saved_position_ms) >= 5_000 {
                            self.send_backend(BackendCommand::SavePlaybackPosition {
                                spotify_uri,
                                position_ms,
                            });
                            self.last_saved_position_ms = position_ms;
                        }
                    }
                }
                BackendEvent::PlaybackSnapshotLoaded {
                    current,
                    next,
                    position_ms,
                } => {
                    self.playback_context = std::iter::once(current.clone())
                        .chain(next.iter().cloned())
                        .collect::<Vec<_>>()
                        .into();
                    self.now_playing = Some(current);
                    self.queue = next.into();
                    self.position_ms = position_ms;
                    self.last_saved_position_ms = position_ms;
                    self.playing = false;
                    self.playback_loading = false;
                }
                BackendEvent::PlaybackContext { current, next } => {
                    let changed = self.now_playing.as_ref().is_none_or(|track| {
                        track.provider != current.provider || track.source_id != current.source_id
                    });
                    self.playback_context = std::iter::once(current.clone())
                        .chain(next.iter().cloned())
                        .collect::<Vec<_>>()
                        .into();
                    self.now_playing = Some(current);
                    self.queue = next.into();
                    if changed {
                        self.playback_loading = true;
                        self.position_ms = 0;
                        self.last_saved_position_ms = 0;
                        self.playback_restore = None;
                    }
                }
                BackendEvent::AuthorizationFailed(error) => {
                    self.connection_state = ConnectionState::AuthorizationRequired;
                    self.last_error = Some(error);
                }
                BackendEvent::CatalogFailed { generation, error } => {
                    if generation == self.account_generation {
                        self.connection_state = ConnectionState::Ready;
                        self.library_loaded = true;
                        self.last_error = Some(error);
                    }
                }
                BackendEvent::PlaybackFailed(error) => {
                    self.playback_error = Some(error);
                }
                BackendEvent::TrackFailed { spotify_uri, error } => {
                    if self.live_track_matches(&spotify_uri) {
                        self.now_playing = None;
                        self.playback_context = Arc::default();
                        self.queue = Arc::default();
                        self.playing = false;
                        self.playback_loading = false;
                    }
                    self.last_error = Some(error);
                }
                BackendEvent::RadioFailed { request_id, error } => {
                    if self.pending_radio_request == Some(request_id) {
                        self.pending_radio_request = None;
                        self.playback_loading = false;
                        self.action_notice = Some(format!("Track radio unavailable: {error}"));
                    }
                }
                BackendEvent::RadioStarted { request_id } => {
                    if self.pending_radio_request == Some(request_id) {
                        self.pending_radio_request = None;
                        self.action_notice = None;
                    }
                }
                BackendEvent::Error(error) => {
                    self.last_error = Some(error);
                }
            }
        }
        cx.notify();
    }
}
