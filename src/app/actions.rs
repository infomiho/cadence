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
        if self.session.read(cx).app_change_confirmation_open() {
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
        self.last_error = None;
        cx.notify();
    }

    pub(super) fn authenticate(&mut self, cx: &mut Context<Self>) {
        self.session
            .update(cx, |session, cx| session.authenticate(cx));
    }

    pub(super) fn configure_spotify(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let client_id = self
            .spotify_client_id_input
            .read(cx)
            .value()
            .trim()
            .to_owned();
        if !valid_client_id(&client_id) {
            self.session.update(cx, |session, cx| {
                session.reject_client_id(
                    "Enter the 32-character Client ID from your Spotify app.",
                    cx,
                )
            });
            window.focus(&self.spotify_client_id_input.read(cx).focus_handle(cx));
            cx.notify();
            return;
        }
        self.session
            .update(cx, |session, cx| session.configure(client_id, cx));
        cx.notify();
    }

    pub(super) fn request_spotify_app_change(&mut self, cx: &mut Context<Self>) {
        self.session
            .update(cx, |session, cx| session.request_app_change(cx));
    }

    pub(super) fn cancel_spotify_app_change(&mut self, cx: &mut Context<Self>) {
        self.session
            .update(cx, |session, cx| session.cancel_app_change(cx));
    }

    pub(super) fn confirm_spotify_app_change(&mut self, cx: &mut Context<Self>) {
        self.session
            .update(cx, |session, cx| session.confirm_app_change(cx));
    }

    pub(super) fn logout(&mut self, cx: &mut Context<Self>) {
        self.session.update(cx, |session, cx| session.logout(cx));
    }

    /// Drops every in-flight catalog reply and returns to the top of the app,
    /// for when the account behind them is changing.
    pub(super) fn restart_navigation(&mut self, cx: &mut Context<Self>) {
        self.route = Route::LikedSongs;
        next_request_id(&mut self.search_request_id);
        next_request_id(&mut self.playlist_request_id);
        next_request_id(&mut self.artist_request_id);
        next_request_id(&mut self.album_request_id);
        self.pending_radio_request = None;
        cx.notify();
    }

    pub(super) fn handle_session_event(
        &mut self,
        event: &session::SessionEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            session::SessionEvent::Restarted => self.restart_navigation(cx),
            session::SessionEvent::Ready => {
                self.last_error = None;
                cx.notify();
            }
            session::SessionEvent::LoggedOut => {
                self.restart_navigation(cx);
                self.clear_account_data(cx);
            }
            session::SessionEvent::Failed(error) => {
                self.last_error = Some(error.clone());
                cx.notify();
            }
            session::SessionEvent::Notice(notice) => {
                self.action_notice = Some(notice.clone());
                cx.notify();
            }
        }
    }

    /// Forgets everything cached for the account that just went away.
    pub(super) fn clear_account_data(&mut self, cx: &mut Context<Self>) {
        self.liked_tracks = Arc::default();
        self.spotify_playlists = Arc::default();
        self.library_loaded = false;
        self.search_results = Arc::default();
        self.search_playlists = Arc::default();
        self.search_loaded = false;
        self.searching = false;
        self.search_error = None;
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
        self.player.update(cx, |player, cx| player.clear(cx));
        self.last_error = None;
        self.action_notice = None;
        cx.notify();
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
        self.session
            .update(cx, |session, cx| session.cancel_app_change(cx));
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
