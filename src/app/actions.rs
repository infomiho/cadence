use super::*;

impl CadenceApp {
    pub(super) fn on_tab(&mut self, _: &Tab, window: &mut Window, _: &mut Context<Self>) {
        window.focus_next();
    }

    pub(super) fn on_tab_prev(&mut self, _: &TabPrev, window: &mut Window, _: &mut Context<Self>) {
        window.focus_prev();
    }

    pub(super) fn open_search(
        &mut self,
        _: &OpenSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.search_input.read(cx).focus_handle(cx));
        self.navigate(Route::Search, cx);
    }

    pub(super) fn close_window(
        &mut self,
        _: &CloseWindow,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.remove_window();
    }

    pub(super) fn dismiss_overlay(
        &mut self,
        _: &DismissOverlay,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.spotify_app_change_confirmation_open {
            self.cancel_spotify_app_change(cx);
        }
    }

    pub(super) fn toggle_playback(
        &mut self,
        _: &TogglePlayback,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_focused_input(cx) {
            return;
        }
        self.player.update(cx, |player, cx| player.toggle(cx));
    }

    /// Starts `tracks` at `index`, reporting whether playback was accepted.
    pub(super) fn play_context(
        &mut self,
        tracks: Vec<model::Track>,
        index: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        self.player
            .update(cx, |player, cx| player.play_context(tracks, index, cx))
    }

    pub(super) fn send_backend(&mut self, command: BackendCommand) -> bool {
        if self.backend.send(command) {
            true
        } else {
            self.last_error = Some("Cadence backend is busy or not running".to_owned());
            false
        }
    }

    pub(super) fn retry_backend(&mut self, cx: &mut Context<Self>) {
        let (backend, backend_events) = services::AppServices::restart(cx);
        self.backend = backend;
        Self::observe_backend_events(backend_events, cx);
        self.connection_state = ConnectionState::Starting;
        self.last_error = None;
        cx.notify();
    }

    pub(super) fn authenticate(&mut self) {
        self.route = Route::LikedSongs;
        let generation = next_request_id(&mut self.account_generation);
        self.send_backend(BackendCommand::Authenticate { generation });
    }

    pub(super) fn configure_spotify(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let client_id = self
            .spotify_client_id_input
            .read(cx)
            .value()
            .trim()
            .to_owned();
        if !valid_client_id(&client_id) {
            self.spotify_setup_error =
                Some("Enter the 32-character Client ID from your Spotify app.".to_owned());
            window.focus(&self.spotify_client_id_input.read(cx).focus_handle(cx));
            cx.notify();
            return;
        }

        self.spotify_setup_error = None;
        self.connection_state = ConnectionState::Connecting;
        let generation = next_request_id(&mut self.spotify_configuration_request_id);
        self.pending_spotify_configuration = Some(generation);
        if !self.send_backend(BackendCommand::ConfigureSpotify {
            generation,
            client_id,
        }) {
            self.pending_spotify_configuration = None;
            self.connection_state = ConnectionState::SetupRequired;
        }
        cx.notify();
    }

    pub(super) fn request_spotify_app_change(&mut self, cx: &mut Context<Self>) {
        self.spotify_app_change_confirmation_open = true;
        cx.notify();
    }

    pub(super) fn cancel_spotify_app_change(&mut self, cx: &mut Context<Self>) {
        self.spotify_app_change_confirmation_open = false;
        cx.notify();
    }

    pub(super) fn confirm_spotify_app_change(&mut self, cx: &mut Context<Self>) {
        self.spotify_app_change_confirmation_open = false;
        self.route = Route::LikedSongs;
        self.connection_state = ConnectionState::Connecting;
        let generation = next_request_id(&mut self.account_generation);
        next_request_id(&mut self.search_request_id);
        next_request_id(&mut self.playlist_request_id);
        next_request_id(&mut self.artist_request_id);
        next_request_id(&mut self.album_request_id);
        self.pending_radio_request = None;
        if !self.send_backend(BackendCommand::ResetSpotifyConfiguration { generation }) {
            self.connection_state = ConnectionState::Ready;
            self.last_error = Some("Unable to restart Spotify setup.".to_owned());
        }
        cx.notify();
    }

    pub(super) fn logout(&mut self) {
        self.route = Route::LikedSongs;
        self.spotify_app_change_confirmation_open = false;
        let generation = next_request_id(&mut self.account_generation);
        next_request_id(&mut self.search_request_id);
        next_request_id(&mut self.playlist_request_id);
        next_request_id(&mut self.artist_request_id);
        next_request_id(&mut self.album_request_id);
        self.pending_radio_request = None;
        self.send_backend(BackendCommand::Logout { generation });
    }

    pub(super) fn load_playlist(&mut self, playlist: model::Playlist) {
        let request_id = next_request_id(&mut self.playlist_request_id);
        self.send_backend(BackendCommand::LoadPlaylist {
            request_id,
            playlist,
        });
    }

    pub(super) fn submit_search(&mut self, cx: &mut Context<Self>) {
        let query = self.search_query.trim().to_owned();
        if query.is_empty() {
            return;
        }
        self.search_loaded = false;
        self.searching = true;
        self.search_error = None;
        self.last_error = None;
        let request_id = next_request_id(&mut self.search_request_id);
        if !self.send_backend(BackendCommand::SearchCatalog { request_id, query }) {
            self.search_loaded = true;
            self.searching = false;
        }
        self.navigate(Route::Search, cx);
    }

    pub(super) fn end_volume_drag(
        &mut self,
        _: &gpui::MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.player
            .update(cx, |player, cx| player.end_volume_drag(cx));
    }

    /// Hides the queue panel, which the player bar's toggle also reflects.
    pub(super) fn close_queue(&mut self, cx: &mut Context<Self>) {
        self.player_bar
            .update(cx, |bar, cx| bar.set_queue_open(false, cx));
        cx.notify();
    }

    pub(super) fn navigate(&mut self, route: Route, cx: &mut Context<Self>) {
        self.route = route;
        self.close_queue(cx);
        self.account_menu_open = false;
        self.track_menu_open = None;
        self.spotify_app_change_confirmation_open = false;
        cx.notify();
    }

    pub(super) fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_origin = match self.route {
            Route::Settings => self.settings_origin,
            Route::Playlist => self.playlist_origin,
            Route::Artist => self.artist_origin,
            Route::Album => self.album_origin,
            route => route,
        };
        self.navigate(Route::Settings, cx);
    }

    pub(super) fn open_playlist(&mut self, origin: Route, cx: &mut Context<Self>) {
        self.playlist_origin = origin;
        self.navigate(Route::Playlist, cx);
    }

    pub(super) fn open_artist(
        &mut self,
        artist: model::ArtistRef,
        origin: Route,
        cx: &mut Context<Self>,
    ) {
        let Some(source_id) = artist.source_id.clone() else {
            return;
        };
        let same_artist = self
            .selected_artist_ref
            .as_ref()
            .and_then(|artist| artist.source_id.as_deref())
            == Some(source_id.as_str());
        let retrying_failure = same_artist && self.artist_error.is_some();
        let should_refresh =
            !same_artist || (!self.artist_loading && !catalog_data_is_fresh(self.artist_loaded_at));
        self.selected_artist_ref = Some(artist);
        self.artist_error = None;
        if !same_artist {
            self.selected_artist = None;
            self.artist_tracks = Arc::default();
            self.artist_albums = Arc::default();
            self.artist_loaded = false;
            self.artist_loading = false;
            self.artist_loaded_at = None;
            self.artist_section = ArtistSection::Popular;
        } else if retrying_failure {
            self.artist_loaded = false;
        }
        if !same_artist || self.route != Route::Artist {
            self.artist_origin = origin;
        }
        if should_refresh {
            let request_id = next_request_id(&mut self.artist_request_id);
            self.artist_loading = true;
            if !self.send_backend(BackendCommand::LoadArtist {
                request_id,
                source_id,
            }) {
                self.artist_loading = false;
                self.artist_loaded = true;
                self.artist_error = Some("Cadence backend is not running".to_owned());
            }
        }
        self.navigate(Route::Artist, cx);
    }

    pub(super) fn open_album(
        &mut self,
        album: model::AlbumRef,
        origin: Route,
        cx: &mut Context<Self>,
    ) {
        let Some(source_id) = album.source_id.clone() else {
            return;
        };
        let same_album = self
            .selected_album_ref
            .as_ref()
            .and_then(|album| album.source_id.as_deref())
            == Some(source_id.as_str());
        let retrying_failure = same_album && self.album_error.is_some();
        let should_refresh =
            !same_album || (!self.album_loading && !catalog_data_is_fresh(self.album_loaded_at));
        self.selected_album_ref = Some(album);
        self.album_error = None;
        if !same_album {
            self.selected_album = None;
            self.album_tracks = Arc::default();
            self.album_loaded = false;
            self.album_loading = false;
            self.album_loaded_at = None;
        } else if retrying_failure {
            self.album_loaded = false;
        }
        if !same_album || self.route != Route::Album {
            self.album_origin = origin;
        }
        if should_refresh {
            let request_id = next_request_id(&mut self.album_request_id);
            self.album_loading = true;
            if !self.send_backend(BackendCommand::LoadAlbum {
                request_id,
                source_id,
            }) {
                self.album_loading = false;
                self.album_loaded = true;
                self.album_error = Some("Cadence backend is not running".to_owned());
            }
        }
        self.navigate(Route::Album, cx);
    }
}
