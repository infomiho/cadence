use super::*;

/// The transport strip pinned to the bottom of the window.
///
/// It redraws because it reads the `Player` entity, which gpui tracks per
/// window. Adding `.cached(..)` here would break that: `Player` is a model and
/// has no dispatch node, so it cannot mark this view dirty on its own.
pub(super) struct PlayerBar {
    player: Entity<player::Player>,
    image_cache: Entity<image_cache::BoundedImageCache>,
    queue_open: bool,
}

/// Raised when the listener asks to see or hide the queue.
pub(super) struct ToggleQueue;

impl EventEmitter<ToggleQueue> for PlayerBar {}

impl PlayerBar {
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            player: services::AppServices::player(cx),
            image_cache: services::AppServices::image_cache(cx),
            queue_open: false,
        }
    }

    pub(super) fn queue_open(&self) -> bool {
        self.queue_open
    }

    pub(super) fn set_queue_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.queue_open != open {
            self.queue_open = open;
            cx.notify();
        }
    }

    fn bar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::Appearance::palette(cx);
        let image_cache = self.image_cache.clone();
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
                        components::artwork(
                            palette,
                            &image_cache,
                            player_artwork.as_deref(),
                            56.,
                            12.,
                            "music.note",
                        )
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
                            .child(
                                components::icon_button(palette, "previous", "backward.end.fill")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.player.update(cx, |player, cx| player.previous(cx));
                                    })),
                            )
                            .child(
                                components::button(palette, "play-toggle")
                                    .size(px(40.))
                                    .rounded(px(20.))
                                    .bg(rgb(palette.text_primary))
                                    .child(if loading {
                                        Spinner::new()
                                            .color(rgb(palette.on_accent).into())
                                            .into_any_element()
                                    } else {
                                        components::icon(
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
                            .child(
                                components::icon_button(palette, "next", "forward.end.fill")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.player.update(cx, |player, cx| player.next(cx));
                                    })),
                            ),
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
                        components::icon_button_with(
                            palette,
                            "queue-toggle",
                            "music.note.list",
                            17.,
                            SymbolWeight::Semibold,
                        )
                        .when(self.queue_open, |button| button.bg(rgb(palette.selection)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            let open = !this.queue_open;
                            this.set_queue_open(open, cx);
                            cx.emit(ToggleQueue);
                        })),
                    )
                    .child(
                        components::icon_button_with(
                            palette,
                            "volume",
                            volume_icon,
                            17.,
                            SymbolWeight::Semibold,
                        )
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

impl Render for PlayerBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.bar(window, cx)
    }
}

/// Raised when the listener dismisses the queue panel.
pub(super) struct CloseQueue;

impl EventEmitter<CloseQueue> for QueueDrawer {}

/// The slide-over queue panel.
pub(super) struct QueueDrawer {
    player: Entity<player::Player>,
    image_cache: Entity<image_cache::BoundedImageCache>,
}

impl QueueDrawer {
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            player: services::AppServices::player(cx),
            image_cache: services::AppServices::image_cache(cx),
        }
    }

    fn drawer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::Appearance::palette(cx);
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
                        components::icon_button(palette, "close-queue", "xmark")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(CloseQueue))),
                    ),
            )
            .child(components::section_label(palette, "Now playing"))
            .child(
                now_playing
                    .map(|track| {
                        self.row(palette, "queue-current", track, true)
                            .into_any_element()
                    })
                    .unwrap_or_else(|| {
                        components::empty_state(palette, "Nothing playing").into_any_element()
                    }),
            )
            .child(div().h(px(24.)))
            .child(components::section_label(palette, "Next"))
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
                                        this.row(palette, ("queue-track", index), track, false)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.player.update(cx, |player, cx| {
                                                    player.play_context(
                                                        playback_context.to_vec(),
                                                        index + context_offset,
                                                        cx,
                                                    )
                                                });
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

    fn row(
        &self,
        palette: CadencePalette,
        id: impl Into<ElementId>,
        track: model::Track,
        current: bool,
    ) -> Stateful<Div> {
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
            .child(components::artwork(
                palette,
                &self.image_cache,
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
}

impl Render for QueueDrawer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.drawer(cx)
    }
}
