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

    pub(super) fn retry_backend(&mut self, cx: &mut Context<Self>) {
        self.backend = services::AppServices::restart(cx);
        let backend = self.backend.clone();
        self.search
            .update(cx, |page, _| page.connect(backend.clone()));
        self.playlist
            .update(cx, |page, _| page.connect(backend.clone()));
        self.artist
            .update(cx, |page, _| page.connect(backend.clone()));
        self.album.update(cx, |page, _| page.connect(backend));
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
        self.clear_pages(cx);
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

    /// Drops every page's contents and any request still in flight, for when
    /// the account they were fetched for is changing.
    fn clear_pages(&mut self, cx: &mut Context<Self>) {
        self.search.update(cx, |page, cx| page.clear(cx));
        self.playlist.update(cx, |page, cx| page.clear(cx));
        self.artist.update(cx, |page, cx| page.clear(cx));
        self.album.update(cx, |page, cx| page.clear(cx));
    }

    /// Forgets everything cached for the account that just went away.
    pub(super) fn clear_account_data(&mut self, cx: &mut Context<Self>) {
        self.library.update(cx, |library, cx| library.clear(cx));
        self.clear_pages(cx);
        self.player.update(cx, |player, cx| player.clear(cx));
        self.last_error = None;
        self.action_notice = None;
        cx.notify();
    }

    pub(super) fn load_playlist(&mut self, playlist: model::Playlist, cx: &mut Context<Self>) {
        self.playlist.update(cx, |page, cx| page.open(playlist, cx));
    }

    pub(super) fn submit_search(&mut self, cx: &mut Context<Self>) {
        if !self.search.update(cx, |search, cx| search.submit(cx)) {
            return;
        }
        self.last_error = None;
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
        if artist.source_id.is_none() {
            return;
        }
        let changed = self.artist.update(cx, |page, cx| page.open(artist, cx));
        if changed || self.route != Route::Artist {
            self.artist_origin = origin;
        }
        self.navigate(Route::Artist, cx);
    }

    pub(super) fn open_album(
        &mut self,
        album: model::AlbumRef,
        origin: Route,
        cx: &mut Context<Self>,
    ) {
        if album.source_id.is_none() {
            return;
        }
        let changed = self.album.update(cx, |page, cx| page.open(album, cx));
        if changed || self.route != Route::Album {
            self.album_origin = origin;
        }
        self.navigate(Route::Album, cx);
    }

    pub(super) fn handle_page_event(
        &mut self,
        _: Entity<impl EventEmitter<catalog::PageEvent> + 'static>,
        event: &catalog::PageEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            catalog::PageEvent::Loaded => self.last_error = None,
            catalog::PageEvent::Failed(error) => self.last_error = Some(error.clone()),
        }
        cx.notify();
    }
}
