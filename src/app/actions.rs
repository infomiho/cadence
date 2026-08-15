use super::*;

impl CadenceApp {
    pub(super) fn live_track_matches(&self, spotify_uri: &str) -> bool {
        self.now_playing
            .as_ref()
            .and_then(|track| track.spotify_uri.as_deref())
            == Some(spotify_uri)
    }

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

    pub(super) fn toggle_playback(
        &mut self,
        _: &TogglePlayback,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_focused_input(cx) || self.now_playing.is_none() {
            return;
        }
        let playing = !self.playing;
        if self.send_backend(if self.playing {
            BackendCommand::Pause
        } else {
            BackendCommand::Resume
        }) {
            self.playing = playing;
            self.playback_loading = playing;
            cx.notify();
        }
    }

    pub(super) fn send_backend(&mut self, command: BackendCommand) -> bool {
        if self.playback_restore.is_some()
            && matches!(
                &command,
                BackendCommand::PlayContext { .. }
                    | BackendCommand::Next
                    | BackendCommand::Previous
                    | BackendCommand::Pause
                    | BackendCommand::Resume
                    | BackendCommand::Seek(_)
            )
        {
            return false;
        }
        if self.backend.send(command) {
            true
        } else {
            self.last_error = Some("Cadence backend is not running".to_owned());
            false
        }
    }

    pub(super) fn authenticate(&mut self) {
        let generation = next_request_id(&mut self.account_generation);
        self.send_backend(BackendCommand::Authenticate { generation });
    }

    pub(super) fn logout(&mut self) {
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

    pub(super) fn update_volume_from_pointer(&mut self, pointer_x: Pixels, window: &Window) {
        let window_width = f32::from(window.window_bounds().get_bounds().size.width);
        self.volume = volume_for_pointer(f32::from(pointer_x), window_width);
        if self.volume > 0. {
            self.previous_volume = self.volume;
        }
    }

    pub(super) fn end_volume_drag(
        &mut self,
        _: &gpui::MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.volume_dragging {
            self.volume_dragging = false;
            cx.notify();
        }
    }

    pub(super) fn navigate(&mut self, route: Route, cx: &mut Context<Self>) {
        self.route = route;
        self.queue_open = false;
        self.account_menu_open = false;
        self.track_menu_open = None;
        cx.notify();
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
