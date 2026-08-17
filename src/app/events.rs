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
        let search = self.search.clone();
        let playlist = self.playlist.clone();
        let artist = self.artist.clone();
        let album = self.album.clone();
        for event in events {
            let generation = self.session.read(cx).generation();
            let Some(event) = search.update(cx, |page, cx| {
                page.handle_backend_event(event, generation, cx)
            }) else {
                continue;
            };
            let Some(event) = playlist.update(cx, |page, cx| {
                page.handle_backend_event(event, generation, cx)
            }) else {
                continue;
            };
            let Some(event) = artist.update(cx, |page, cx| {
                page.handle_backend_event(event, generation, cx)
            }) else {
                continue;
            };
            let Some(event) = album.update(cx, |page, cx| {
                page.handle_backend_event(event, generation, cx)
            }) else {
                continue;
            };
            match event {
                BackendEvent::CatalogFailed { generation, error } => {
                    if generation == self.session.read(cx).generation() {
                        self.library
                            .update(cx, |library, cx| library.mark_loaded(cx));
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
                // Consumed by the app-scoped services before the window sees them.
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
                | BackendEvent::LibraryLoaded { .. }
                | BackendEvent::CachedLikedTracks { .. }
                | BackendEvent::LocalStateLoaded { .. }
                | BackendEvent::SearchResults { .. }
                | BackendEvent::SearchFailed { .. }
                | BackendEvent::PlaylistLoaded { .. }
                | BackendEvent::PlaylistFailed { .. }
                | BackendEvent::ArtistLoaded { .. }
                | BackendEvent::ArtistFailed { .. }
                | BackendEvent::AlbumLoaded { .. }
                | BackendEvent::AlbumFailed { .. }
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
