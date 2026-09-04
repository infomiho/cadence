use super::*;

impl Workspace {
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
                | BackendEvent::CachedLibrary { .. }
                | BackendEvent::LibraryUnchanged { .. }
                | BackendEvent::LocalStateLoaded { .. }
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
