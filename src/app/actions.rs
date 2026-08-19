use super::*;

impl Workspace {
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
        self.toolbar
            .update(cx, |toolbar, cx| toolbar.focus_search(window, cx));
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

    pub(super) fn retry_backend(&mut self, cx: &mut Context<Self>) {
        services::AppServices::restart(cx);
        self.last_error = None;
        cx.notify();
    }

    pub(super) fn authenticate(&mut self, cx: &mut Context<Self>) {
        self.session
            .update(cx, |session, cx| session.authenticate(cx));
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
    pub(super) fn restart_navigation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Route::LikedSongs, cx);
        self.clear_pages(window, cx);
        self.pending_radio_request = None;
    }

    pub(super) fn handle_session_event(
        &mut self,
        event: &session::SessionEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            session::SessionEvent::Restarted => self.restart_navigation(window, cx),
            session::SessionEvent::Ready => {
                self.last_error = None;
                cx.notify();
            }
            session::SessionEvent::LoggedOut => {
                self.restart_navigation(window, cx);
                self.clear_account_data(window, cx);
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

    /// Drops every page's contents and any request still in flight.
    fn clear_pages(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.toolbar
            .update(cx, |toolbar, cx| toolbar.clear_search(window, cx));
        self.search.update(cx, |page, cx| page.clear(cx));
        self.playlist.update(cx, |page, cx| page.clear(cx));
        self.artist.update(cx, |page, cx| page.clear(cx));
        self.album.update(cx, |page, cx| page.clear(cx));
    }

    /// Forgets everything cached for the account that just went away.
    pub(super) fn clear_account_data(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.library.update(cx, |library, cx| library.clear(cx));
        self.clear_pages(window, cx);
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

    /// Row action menus are anchored to a page that a route change takes off
    /// screen, so they have to come down with it or they reappear on return.
    pub(super) fn close_track_menus(&mut self, cx: &mut Context<Self>) {
        self.liked_songs.update(cx, |page, cx| page.close_menus(cx));
        self.favorites.update(cx, |page, cx| page.close_menus(cx));
        self.recent.update(cx, |page, cx| page.close_menus(cx));
        self.search.update(cx, |page, cx| page.close_menus(cx));
        self.playlist.update(cx, |page, cx| page.close_menus(cx));
        self.artist.update(cx, |page, cx| page.close_menus(cx));
        self.album.update(cx, |page, cx| page.close_menus(cx));
    }

    pub(super) fn close_account_menu(&mut self, cx: &mut Context<Self>) {
        self.toolbar
            .update(cx, |toolbar, cx| toolbar.close_menu(cx));
    }

    pub(super) fn navigate(&mut self, route: Route, cx: &mut Context<Self>) {
        self.router.navigate(route);
        self.settle_navigation(cx);
    }

    pub(super) fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.router.open_settings();
        self.settle_navigation(cx);
    }

    pub(super) fn open_playlist(&mut self, origin: Route, cx: &mut Context<Self>) {
        self.router.open_playlist(origin);
        self.settle_navigation(cx);
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
        self.router.open_artist(origin, changed);
        self.settle_navigation(cx);
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
        self.router.open_album(origin, changed);
        self.settle_navigation(cx);
    }

    /// Clears everything overlaying the page and tells the sidebar where the
    /// listener ended up, whichever way the route changed.
    fn settle_navigation(&mut self, cx: &mut Context<Self>) {
        let route = self.router.route();
        let pinned_origin = self.router.pinned_origin();
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.show_route(route, pinned_origin, cx)
        });
        self.close_queue(cx);
        self.close_account_menu(cx);
        self.close_track_menus(cx);
        self.session
            .update(cx, |session, cx| session.cancel_app_change(cx));
        cx.notify();
    }

    pub(super) fn handle_page_event(
        &mut self,
        _: Entity<impl EventEmitter<page::PageEvent> + 'static>,
        event: &page::PageEvent,
        cx: &mut Context<Self>,
    ) {
        let origin = self.router.route();
        match event {
            page::PageEvent::Loaded => self.last_error = None,
            page::PageEvent::Failed(error) => self.last_error = Some(error.clone()),
            page::PageEvent::OpenPlaylist(playlist) => {
                self.load_playlist(playlist.clone(), cx);
                self.open_playlist(origin, cx);
            }
            page::PageEvent::OpenArtist(artist) => self.open_artist(artist.clone(), origin, cx),
            page::PageEvent::OpenAlbum(album) => self.open_album(album.clone(), origin, cx),
            page::PageEvent::StartRadio(track) => self.start_track_radio(track.clone(), cx),
        }
        cx.notify();
    }

    /// Queues a radio seeded from `track`, tracking the request so a later
    /// failure or cancellation can clear the notice it puts up.
    fn start_track_radio(&mut self, track: model::Track, cx: &mut Context<Self>) {
        self.action_notice = Some("Starting track radio…".to_owned());
        let request_id = next_request_id(&mut self.radio_request_id);
        self.pending_radio_request = Some(request_id);
        let started = self
            .player
            .update(cx, |player, cx| player.start_radio(request_id, track, cx));
        if !started {
            self.pending_radio_request = None;
            self.action_notice = Some("Unable to start track radio".to_owned());
        }
    }
}
