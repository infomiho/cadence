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

    /// Rebinds to a replacement backend after the previous worker was restarted.
    pub(super) fn connect(&mut self, backend: BackendHandle) {
        self.backend = backend;
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

impl CadenceApp {
    pub(super) fn queue_drawer(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let player = self.player.read(cx);
        let queue = player.queue().clone();
        let queue_count = queue.len();
        let context_offset = usize::from(player.now_playing().is_some());
        let playback_context = player.context().clone();
        let now_playing = player.now_playing().cloned();

        div()
            .occlude()
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .w(px(420.))
            .p(px(24.))
            .bg(rgb(palette.surface))
            .border_l_1()
            .border_color(rgb(palette.border))
            .shadow_xl()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb(px(24.))
                    .child(
                        div()
                            .text_size(px(32.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(palette.text_primary))
                            .child("Queue"),
                    )
                    .child(
                        self.icon_button("close-queue", "xmark")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.queue_open = false;
                                cx.notify();
                            })),
                    ),
            )
            .child(self.section_label("Now playing"))
            .child(
                now_playing
                    .map(|track| {
                        self.queue_row("queue-current", track, true)
                            .into_any_element()
                    })
                    .unwrap_or_else(|| self.empty_state("Nothing playing").into_any_element()),
            )
            .child(div().h(px(24.)))
            .child(self.section_label("Next"))
            .child(
                div()
                    .id("queue-scroll")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(
                        uniform_list(
                            "queue-tracks",
                            queue_count,
                            cx.processor(move |this, range: Range<usize>, _, cx| {
                                range
                                    .map(|index| {
                                        let track = queue[index].clone();
                                        let playback_context = playback_context.clone();
                                        this.queue_row(("queue-track", index), track, false)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.play_context(
                                                    playback_context.to_vec(),
                                                    index + context_offset,
                                                    cx,
                                                );
                                            }))
                                            .into_any_element()
                                    })
                                    .collect()
                            }),
                        )
                        .flex_1()
                        .min_h_0(),
                    ),
            )
    }

    pub(super) fn queue_row(
        &self,
        id: impl Into<ElementId>,
        track: model::Track,
        current: bool,
    ) -> Stateful<Div> {
        let palette = self.palette;
        div()
            .id(id)
            .w_full()
            .h(px(if current { 72. } else { 62. }))
            .flex_none()
            .mt(px(8.))
            .px(px(10.))
            .rounded(px(16.))
            .bg(if current {
                rgb(palette.selection)
            } else {
                rgb(palette.surface)
            })
            .when(!current, |row| {
                row.cursor_pointer()
                    .hover(|style| style.bg(rgb(palette.surface_hover)))
            })
            .flex()
            .items_center()
            .gap(px(12.))
            .child(self.artwork(
                track.artwork_url.as_deref(),
                if current { 48. } else { 40. },
                8.,
                "music.note",
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .w_full()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(palette.text_primary))
                            .child(track.title.clone()),
                    )
                    .child(
                        div()
                            .w_full()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(12.))
                            .text_color(rgb(if current {
                                palette.text
                            } else {
                                palette.text_muted
                            }))
                            .child(track.artist.clone()),
                    ),
            )
            .child(
                div()
                    .w(px(44.))
                    .flex_none()
                    .text_right()
                    .text_size(px(12.))
                    .text_color(rgb(palette.text_muted))
                    .child(format_duration(track.duration_ms)),
            )
    }

    pub(super) fn player_bar(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.palette;
        let compact = uses_compact_player_layout(f32::from(window.viewport_size().width));
        let progress_slider_width = if compact {
            (f32::from(window.viewport_size().width) - 500.).max(160.)
        } else {
            PROGRESS_SLIDER_WIDTH
        };
        let player = self.player.read(cx);
        let now_playing = player.now_playing().cloned();
        let playing = player.playing();
        let loading = player.loading();
        let position_ms = player.position_ms();
        let volume = player.volume();
        let live_track = now_playing.is_some();
        let player_artwork = now_playing
            .as_ref()
            .and_then(|track| track.artwork_url.as_deref())
            .map(str::to_owned);
        let (title, artist, duration, art) = if let Some(track) = &now_playing {
            (
                SharedString::from(track.title.clone()),
                SharedString::from(track.artist.clone()),
                SharedString::from(format_duration(track.duration_ms)),
                palette.selection,
            )
        } else {
            (
                SharedString::from("Nothing playing"),
                SharedString::from(""),
                SharedString::from("0:00"),
                palette.surface_raised,
            )
        };
        let volume_icon = if volume == 0. {
            "speaker.slash.fill"
        } else {
            "speaker.wave.2.fill"
        };
        let duration_ms = now_playing.as_ref().map_or(0, |track| track.duration_ms);
        let progress = if duration_ms == 0 {
            0.
        } else {
            (position_ms as f32 / duration_ms as f32).clamp(0., 1.)
        };
        div()
            .h(px(96.))
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(24.))
            .px(px(24.))
            .bg(rgb(palette.surface))
            .border_t_1()
            .border_color(rgb(palette.border))
            .child(
                div()
                    .w(px(if compact {
                        COMPACT_PLAYER_LEFT_WIDTH
                    } else {
                        PLAYER_LEFT_WIDTH
                    }))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(if live_track {
                        self.artwork(player_artwork.as_deref(), 56., 12., "music.note")
                    } else {
                        div()
                            .size(px(56.))
                            .rounded(px(12.))
                            .bg(rgb(art))
                            .border_1()
                            .border_color(palette.media_border)
                            .into_any_element()
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(palette.text_primary))
                                    .child(title),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.))
                                    .text_color(rgb(palette.text_muted))
                                    .child(artist),
                            ),
                    ),
            )
            .child(
                div()
                    .w(px(if compact {
                        progress_slider_width + 2. * PROGRESS_TIME_WIDTH + 2. * PROGRESS_GAP
                    } else {
                        PLAYER_CENTER_WIDTH
                    }))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(self.icon_button("previous", "backward.end.fill").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.player.update(cx, |player, cx| player.previous(cx));
                                }),
                            ))
                            .child(
                                self.button("play-toggle")
                                    .size(px(40.))
                                    .rounded(px(20.))
                                    .bg(rgb(palette.text_primary))
                                    .child(if loading {
                                        Spinner::new()
                                            .color(rgb(palette.on_accent).into())
                                            .into_any_element()
                                    } else {
                                        Self::icon(
                                            if playing { "pause.fill" } else { "play.fill" },
                                            16.,
                                            palette.on_accent,
                                        )
                                        .into_any_element()
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.player.update(cx, |player, cx| player.toggle(cx));
                                    })),
                            )
                            .child(self.icon_button("next", "forward.end.fill").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.player.update(cx, |player, cx| player.next(cx));
                                }),
                            )),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(11.))
                            .text_color(rgb(palette.text_muted))
                            .child(
                                div()
                                    .w(px(PROGRESS_TIME_WIDTH))
                                    .flex_none()
                                    .text_right()
                                    .child(format_duration(position_ms)),
                            )
                            .child(
                                div()
                                    .id("progress-slider")
                                    .h(px(5.))
                                    .w(px(progress_slider_width))
                                    .flex_none()
                                    .rounded(px(3.))
                                    .bg(rgb(palette.surface_raised))
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(
                                            |this, event: &gpui::MouseDownEvent, window, cx| {
                                                let window_width = f32::from(
                                                    window.window_bounds().get_bounds().size.width,
                                                );
                                                this.player.update(cx, |player, cx| {
                                                    let Some(duration_ms) = player
                                                        .now_playing()
                                                        .map(|track| track.duration_ms)
                                                    else {
                                                        return;
                                                    };
                                                    let position = seek_for_pointer(
                                                        f32::from(event.position.x),
                                                        window_width,
                                                        duration_ms,
                                                    );
                                                    player.seek(position, cx);
                                                });
                                            },
                                        ),
                                    )
                                    .child(
                                        div()
                                            .w(relative(progress))
                                            .h_full()
                                            .rounded(px(3.))
                                            .bg(rgb(palette.text_primary)),
                                    ),
                            )
                            .child(div().w(px(PROGRESS_TIME_WIDTH)).flex_none().child(duration)),
                    ),
            )
            .child(
                div()
                    .w(px(if compact {
                        COMPACT_PLAYER_RIGHT_WIDTH
                    } else {
                        PLAYER_RIGHT_WIDTH
                    }))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        self.icon_button_with(
                            "queue-toggle",
                            "music.note.list",
                            17.,
                            SymbolWeight::Semibold,
                        )
                        .when(self.queue_open, |button| button.bg(rgb(palette.selection)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.queue_open = !this.queue_open;
                            if this.queue_open {
                                this.account_menu_open = false;
                                this.track_menu_open = None;
                            }
                            cx.notify();
                        })),
                    )
                    .child(
                        self.icon_button_with("volume", volume_icon, 17., SymbolWeight::Semibold)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.player.update(cx, |player, cx| player.toggle_mute(cx));
                            })),
                    )
                    .when(!compact, |controls| {
                        controls.child(
                            div()
                                .id("volume-slider")
                                .w(px(VOLUME_SLIDER_WIDTH))
                                .h(px(24.))
                                .flex()
                                .items_center()
                                .cursor_pointer()
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(
                                        |this, event: &gpui::MouseDownEvent, window, cx| {
                                            this.player.update(cx, |player, cx| {
                                                player.begin_volume_drag(
                                                    event.position.x,
                                                    window,
                                                    cx,
                                                );
                                            });
                                        },
                                    ),
                                )
                                .child(
                                    div()
                                        .relative()
                                        .w_full()
                                        .h(px(4.))
                                        .rounded(px(2.))
                                        .bg(rgb(palette.surface_raised))
                                        .child(
                                            div()
                                                .h_full()
                                                .w(px(VOLUME_SLIDER_WIDTH * volume))
                                                .rounded(px(2.))
                                                .bg(rgb(palette.text_primary)),
                                        )
                                        .child(
                                            div()
                                                .absolute()
                                                .left(px((VOLUME_SLIDER_WIDTH - 12.) * volume))
                                                .top(px(-4.))
                                                .size(px(12.))
                                                .rounded(px(6.))
                                                .bg(rgb(palette.text_primary))
                                                .border_2()
                                                .border_color(rgb(palette.surface)),
                                        ),
                                ),
                        )
                    }),
            )
    }
}
