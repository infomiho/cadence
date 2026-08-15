use super::*;

impl CadenceApp {
    pub(super) fn queue_drawer(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let queue = self.queue.clone();
        let queue_count = queue.len();
        let context_offset = usize::from(self.now_playing.is_some());
        let playback_context = self.playback_context.clone();

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
                self.now_playing
                    .clone()
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
                                                if this.send_backend(BackendCommand::PlayContext {
                                                    tracks: playback_context.to_vec(),
                                                    index: index + context_offset,
                                                }) {
                                                    this.position_ms = 0;
                                                    this.playback_loading = true;
                                                }
                                                cx.notify();
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
        let live_track = self.now_playing.is_some();
        let player_artwork = self
            .now_playing
            .as_ref()
            .and_then(|track| track.artwork_url.as_deref());
        let (title, artist, duration, art) = if let Some(track) = &self.now_playing {
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
        let volume_icon = if self.volume == 0. {
            "speaker.slash.fill"
        } else {
            "speaker.wave.2.fill"
        };
        let duration_ms = self
            .now_playing
            .as_ref()
            .map_or(0, |track| track.duration_ms);
        let progress = if duration_ms == 0 {
            0.
        } else {
            (self.position_ms as f32 / duration_ms as f32).clamp(0., 1.)
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
                        self.artwork(player_artwork, 56., 12., "music.note")
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
                                    if this.now_playing.is_some()
                                        && this.send_backend(BackendCommand::Previous)
                                    {
                                        this.playback_loading = true;
                                    }
                                    cx.notify();
                                }),
                            ))
                            .child(
                                self.button("play-toggle")
                                    .size(px(40.))
                                    .rounded(px(20.))
                                    .bg(rgb(palette.text_primary))
                                    .child(if self.playback_loading {
                                        Spinner::new()
                                            .color(rgb(palette.on_accent).into())
                                            .into_any_element()
                                    } else {
                                        Self::icon(
                                            if self.playing {
                                                "pause.fill"
                                            } else {
                                                "play.fill"
                                            },
                                            16.,
                                            palette.on_accent,
                                        )
                                        .into_any_element()
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if this.now_playing.is_some() {
                                            let next_playing = !this.playing;
                                            if this.send_backend(if this.playing {
                                                BackendCommand::Pause
                                            } else {
                                                BackendCommand::Resume
                                            }) {
                                                this.playing = next_playing;
                                                this.playback_loading = next_playing;
                                            }
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(self.icon_button("next", "forward.end.fill").on_click(
                                cx.listener(|this, _, _, cx| {
                                    if this.now_playing.is_some()
                                        && this.send_backend(BackendCommand::Next)
                                    {
                                        this.playback_loading = true;
                                    }
                                    cx.notify();
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
                                    .child(format_duration(self.position_ms)),
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
                                                if let Some(track) = &this.now_playing {
                                                    let window_width = f32::from(
                                                        window
                                                            .window_bounds()
                                                            .get_bounds()
                                                            .size
                                                            .width,
                                                    );
                                                    let position = seek_for_pointer(
                                                        f32::from(event.position.x),
                                                        window_width,
                                                        track.duration_ms,
                                                    );
                                                    if this.send_backend(BackendCommand::Seek(
                                                        position,
                                                    )) {
                                                        this.position_ms = position;
                                                    }
                                                    cx.notify();
                                                }
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
                                if this.volume > 0. {
                                    this.previous_volume = this.volume;
                                    this.volume = 0.;
                                } else {
                                    this.volume = this.previous_volume.max(0.2);
                                }
                                this.send_backend(BackendCommand::SetVolume(this.volume));
                                cx.notify();
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
                                            this.volume_dragging = true;
                                            this.update_volume_from_pointer(
                                                event.position.x,
                                                window,
                                            );
                                            this.send_backend(BackendCommand::SetVolume(
                                                this.volume,
                                            ));
                                            cx.notify();
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
                                                .w(px(VOLUME_SLIDER_WIDTH * self.volume))
                                                .rounded(px(2.))
                                                .bg(rgb(palette.text_primary)),
                                        )
                                        .child(
                                            div()
                                                .absolute()
                                                .left(px((VOLUME_SLIDER_WIDTH - 12.) * self.volume))
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
