use super::*;

/// How far playback may drift from the saved position before it is written back.
const POSITION_SAVE_INTERVAL_MS: u32 = 5_000;

/// Reported when a playback command could not be delivered to the backend.
pub(super) struct PlaybackUnavailable;

impl EventEmitter<PlaybackUnavailable> for Player {}

/// Playback state that belongs to the process rather than to a window.
pub(super) struct Player {
    backend: BackendHandle,
    now_playing: Option<model::Track>,
    context: Arc<[model::Track]>,
    queue: Arc<[model::Track]>,
    playing: bool,
    loading: bool,
    /// Position and play state to reapply once a reconnected player is ready.
    restore: Option<(u32, bool)>,
    position_ms: u32,
    saved_position_ms: u32,
    volume: f32,
    volume_before_mute: f32,
    volume_dragging: bool,
    error: Option<String>,
}

impl Player {
    pub(super) fn new(backend: BackendHandle) -> Self {
        Self {
            backend,
            now_playing: None,
            context: Arc::default(),
            queue: Arc::default(),
            playing: false,
            loading: false,
            restore: None,
            position_ms: 0,
            saved_position_ms: 0,
            volume: 0.72,
            volume_before_mute: 0.72,
            volume_dragging: false,
            error: None,
        }
    }

    pub(super) fn now_playing(&self) -> Option<&model::Track> {
        self.now_playing.as_ref()
    }

    pub(super) fn context(&self) -> &Arc<[model::Track]> {
        &self.context
    }

    pub(super) fn queue(&self) -> &Arc<[model::Track]> {
        &self.queue
    }

    pub(super) fn playing(&self) -> bool {
        self.playing
    }

    pub(super) fn loading(&self) -> bool {
        self.loading
    }

    pub(super) fn position_ms(&self) -> u32 {
        self.position_ms
    }

    pub(super) fn volume(&self) -> f32 {
        self.volume
    }

    pub(super) fn volume_dragging(&self) -> bool {
        self.volume_dragging
    }

    pub(super) fn error(&self) -> Option<&String> {
        self.error.as_ref()
    }

    pub(super) fn is_current_track(&self, track: &model::Track) -> bool {
        self.now_playing.as_ref().is_some_and(|playing| {
            playing.provider == track.provider && playing.source_id == track.source_id
        })
    }

    fn live_track_matches(&self, spotify_uri: &str) -> bool {
        self.now_playing
            .as_ref()
            .and_then(|track| track.spotify_uri.as_deref())
            == Some(spotify_uri)
    }

    /// Playback commands are dropped while a restore is in flight so they cannot
    /// race the position the backend is about to reapply.
    fn send(&self, command: BackendCommand, cx: &mut Context<Self>) -> bool {
        if self.restore.is_some() {
            return false;
        }
        self.deliver(command, cx)
    }

    /// Sends `command` and reports the failure when the backend cannot take it,
    /// so a dead or saturated worker does not leave controls silently inert.
    fn deliver(&self, command: BackendCommand, cx: &mut Context<Self>) -> bool {
        if self.backend.send(command) {
            return true;
        }
        cx.emit(PlaybackUnavailable);
        false
    }

    pub(super) fn toggle(&mut self, cx: &mut Context<Self>) {
        if self.now_playing.is_none() {
            return;
        }
        let playing = !self.playing;
        if self.send(
            if self.playing {
                BackendCommand::Pause
            } else {
                BackendCommand::Resume
            },
            cx,
        ) {
            self.playing = playing;
            self.loading = playing;
        }
        cx.notify();
    }

    /// Moves to `playing`, doing nothing if playback is already in that state.
    pub(super) fn set_playing(&mut self, playing: bool, cx: &mut Context<Self>) {
        if self.playing != playing {
            self.toggle(cx);
        }
    }

    pub(super) fn next(&mut self, cx: &mut Context<Self>) {
        if self.now_playing.is_some() && self.send(BackendCommand::Next, cx) {
            self.loading = true;
        }
        cx.notify();
    }

    pub(super) fn previous(&mut self, cx: &mut Context<Self>) {
        if self.now_playing.is_some() && self.send(BackendCommand::Previous, cx) {
            self.loading = true;
        }
        cx.notify();
    }

    pub(super) fn seek(&mut self, position_ms: u32, cx: &mut Context<Self>) {
        if self.send(BackendCommand::Seek(position_ms), cx) {
            self.position_ms = position_ms;
        }
        cx.notify();
    }

    pub(super) fn play_context(
        &mut self,
        tracks: Vec<model::Track>,
        index: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let started = self.send(BackendCommand::PlayContext { tracks, index }, cx);
        if started {
            self.position_ms = 0;
            self.playing = false;
            self.loading = true;
        }
        cx.notify();
        started
    }

    pub(super) fn play_next(&mut self, track: model::Track, cx: &mut Context<Self>) -> bool {
        self.deliver(BackendCommand::PlayNext(track), cx)
    }

    pub(super) fn append_to_queue(&mut self, track: model::Track, cx: &mut Context<Self>) -> bool {
        self.deliver(BackendCommand::AppendToQueue(track), cx)
    }

    pub(super) fn start_radio(
        &mut self,
        request_id: u64,
        seed: model::Track,
        cx: &mut Context<Self>,
    ) -> bool {
        self.deliver(BackendCommand::StartRadio { request_id, seed }, cx)
    }

    pub(super) fn set_loading(&mut self, loading: bool, cx: &mut Context<Self>) {
        self.loading = loading;
        cx.notify();
    }

    pub(super) fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        if self.volume > 0. {
            self.volume_before_mute = self.volume;
            self.volume = 0.;
        } else {
            self.volume = self.volume_before_mute.max(0.2);
        }
        self.deliver(BackendCommand::SetVolume(self.volume), cx);
        cx.notify();
    }

    pub(super) fn begin_volume_drag(
        &mut self,
        pointer_x: Pixels,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.volume_dragging = true;
        self.drag_volume(pointer_x, window, cx);
    }

    pub(super) fn drag_volume(
        &mut self,
        pointer_x: Pixels,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let window_width = f32::from(window.window_bounds().get_bounds().size.width);
        self.volume = volume_for_pointer(f32::from(pointer_x), window_width);
        if self.volume > 0. {
            self.volume_before_mute = self.volume;
        }
        self.deliver(BackendCommand::SetVolume(self.volume), cx);
        cx.notify();
    }

    pub(super) fn end_volume_drag(&mut self, cx: &mut Context<Self>) {
        if self.volume_dragging {
            self.volume_dragging = false;
            cx.notify();
        }
    }

    /// Writes the live position back so a restart resumes where the listener left off.
    /// The position to persist for the current track, if one is playing.
    pub(super) fn position_snapshot(&self) -> Option<u32> {
        self.now_playing
            .as_ref()
            .and_then(|track| track.spotify_uri.as_ref())
            .map(|_| self.position_ms)
    }

    pub(super) fn save_position(&self) {
        if let Some(spotify_uri) = self
            .now_playing
            .as_ref()
            .and_then(|track| track.spotify_uri.clone())
        {
            self.backend.send(BackendCommand::SavePlaybackPosition {
                spotify_uri,
                position_ms: self.position_ms,
            });
        }
    }

    fn save_position_if_moved(&mut self, spotify_uri: String, position_ms: u32) {
        if position_ms.abs_diff(self.saved_position_ms) < POSITION_SAVE_INTERVAL_MS {
            return;
        }
        self.backend.send(BackendCommand::SavePlaybackPosition {
            spotify_uri,
            position_ms,
        });
        self.saved_position_ms = position_ms;
    }

    fn adopt_context(&mut self, current: model::Track, next: Vec<model::Track>) {
        self.context = std::iter::once(current.clone())
            .chain(next.iter().cloned())
            .collect::<Vec<_>>()
            .into();
        self.now_playing = Some(current);
        self.queue = next.into();
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        self.now_playing = None;
        self.context = Arc::default();
        self.queue = Arc::default();
        self.playing = false;
        self.loading = false;
        self.restore = None;
        self.position_ms = 0;
        self.saved_position_ms = 0;
        self.error = None;
        cx.notify();
    }

    /// Applies the playback half of a backend event, returning the event when the
    /// surrounding app still has its own work to do for it.
    pub(super) fn handle_backend_event(
        &mut self,
        event: BackendEvent,
        cx: &mut Context<Self>,
    ) -> Option<BackendEvent> {
        match event {
            BackendEvent::PlaybackReady => {
                self.error = None;
                self.backend.send(BackendCommand::SetVolume(self.volume));
            }
            BackendEvent::PlaybackReconnecting => {
                if self.restore.is_none() && self.now_playing.is_some() {
                    self.restore = Some((self.position_ms, self.playing));
                }
                self.loading = true;
            }
            BackendEvent::PlaybackReconnected => {
                self.error = None;
                self.backend.send(BackendCommand::SetVolume(self.volume));
                if let Some((position_ms, playing)) = self.restore {
                    self.backend.send(BackendCommand::RestorePlayback {
                        position_ms,
                        playing,
                    });
                } else {
                    self.loading = false;
                }
            }
            BackendEvent::PlaybackRestored {
                position_ms,
                playing,
            } => {
                self.position_ms = position_ms;
                self.saved_position_ms = position_ms;
                self.playing = playing;
                self.loading = false;
                self.restore = None;
            }
            BackendEvent::PlaybackSettled => {
                self.loading = false;
                self.restore = None;
            }
            BackendEvent::QueueEnded => {
                self.playing = false;
                self.loading = false;
            }
            BackendEvent::Playing { spotify_uri } => {
                if self.restore.is_none() && self.live_track_matches(&spotify_uri) {
                    self.playing = true;
                    self.loading = false;
                }
            }
            BackendEvent::Loading { spotify_uri } => {
                if self.restore.is_none() && self.live_track_matches(&spotify_uri) {
                    self.loading = true;
                }
            }
            BackendEvent::Paused { spotify_uri } => {
                if self.restore.is_none() && self.live_track_matches(&spotify_uri) {
                    self.playing = false;
                    self.loading = false;
                    if self.position_ms != self.saved_position_ms {
                        let position_ms = self.position_ms;
                        self.backend.send(BackendCommand::SavePlaybackPosition {
                            spotify_uri,
                            position_ms,
                        });
                        self.saved_position_ms = position_ms;
                    }
                }
            }
            BackendEvent::EndOfTrack { spotify_uri } => {
                if self.restore.is_none() && self.live_track_matches(&spotify_uri) {
                    self.playing = false;
                    self.loading = false;
                    self.backend.send(BackendCommand::Next);
                }
            }
            BackendEvent::PositionChanged {
                spotify_uri,
                position_ms,
            } => {
                if self.restore.is_none() && self.live_track_matches(&spotify_uri) {
                    self.position_ms = position_ms;
                    self.save_position_if_moved(spotify_uri, position_ms);
                }
            }
            BackendEvent::PlaybackSnapshotLoaded {
                current,
                next,
                position_ms,
            } => {
                self.adopt_context(current, next);
                self.position_ms = position_ms;
                self.saved_position_ms = position_ms;
                self.playing = false;
                self.loading = false;
            }
            BackendEvent::PlaybackContext { current, next } => {
                let changed = self.now_playing.as_ref().is_none_or(|track| {
                    track.provider != current.provider || track.source_id != current.source_id
                });
                self.adopt_context(current, next);
                if changed {
                    self.loading = true;
                    self.position_ms = 0;
                    self.saved_position_ms = 0;
                    self.restore = None;
                }
            }
            BackendEvent::PlaybackFailed(error) => {
                self.error = Some(error);
            }
            BackendEvent::TrackFailed { spotify_uri, error } => {
                if self.live_track_matches(&spotify_uri) {
                    self.now_playing = None;
                    self.context = Arc::default();
                    self.queue = Arc::default();
                    self.playing = false;
                    self.loading = false;
                }
                cx.notify();
                return Some(BackendEvent::TrackFailed { spotify_uri, error });
            }
            event => return Some(event),
        }
        cx.notify();
        None
    }
}
