use super::*;
use gpui_kit::TestSupportExt as _;
use gpui_kit::base::ElementExt as _;

const TRACK_HEIGHT: Rems = Rems(0.3125);
const TRACK_HEIGHT_ACTIVE: Rems = Rems(0.4375);
const THUMB_SIZE: Rems = Rems(0.8125);
const THUMB_VISIBLE_SIZE: Pixels = px(0.5);
/// The invisible target around the track. Larger than the 12px Spotify uses and
/// the 16px YouTube uses, so it clears the 24px WCAG 2.5.8 minimum.
const HIT_HEIGHT: Pixels = px(24.);
const VOLUME_TRACK_HEIGHT: Rems = Rems(0.25);
const VOLUME_THUMB_SIZE: Rems = Rems(0.75);
const GROW_DURATION: Duration = Duration::from_millis(160);

const ARROW_STEP_MS: u32 = 5_000;
const PAGE_STEP_MS: u32 = 30_000;

const MASCOT_RISE_DURATION_MS: f32 = 240.;
const MASCOT_TUCK_DURATION_MS: f32 = 150.;
const MASCOT_MIN_TRANSITION_MS: f32 = 60.;

fn compact_progress_slider_width(viewport_width: Pixels, rem_size: Pixels) -> Rems {
    let available = Rems(viewport_width / rem_size) - COMPACT_PLAYER_RESERVED_WIDTH;
    Rems(available.0.max(COMPACT_PROGRESS_SLIDER_MIN_WIDTH.0))
}

fn mascot_transition_duration(playing: bool, remaining: f32) -> Duration {
    let full_duration = if playing {
        MASCOT_RISE_DURATION_MS
    } else {
        MASCOT_TUCK_DURATION_MS
    };
    Duration::from_millis(
        (full_duration * remaining)
            .round()
            .max(MASCOT_MIN_TRANSITION_MS) as u64,
    )
}

/// The transport strip pinned to the bottom of the window.
///
/// It redraws because it reads the `Player` entity, which gpui tracks per
/// window. Adding `.cached(..)` here would break that: `Player` is a model and
/// has no dispatch node, so it cannot mark this view dirty on its own.
pub(super) struct PlayerBar {
    player: Entity<player::Player>,
    image_cache: Entity<image_cache::BoundedImageCache>,
    queue_open: bool,
    mascot_playing: bool,
    mascot_transition_generation: u64,
    mascot_transition_from: f32,
    mascot_transition_duration: Duration,
    mascot_reveal: Rc<Cell<f32>>,
    scrubber: scrubber::Scrubber,
}

/// Raised when the listener asks to see or hide the queue.
pub(super) struct ToggleQueue;

impl EventEmitter<ToggleQueue> for PlayerBar {}

impl EventEmitter<page::PageEvent> for PlayerBar {}

/// The album the playing track came from, when Spotify knows which it is.
fn playing_album(track: Option<&model::Track>) -> Option<model::AlbumRef> {
    track?
        .album_ref
        .clone()
        .filter(|album| album.source_id.is_some())
}

/// Who the bar credits, one entry per artist. A track stored with only its
/// joined artist names credits that whole line as a single plain entry.
fn artist_credits(track: Option<&model::Track>) -> Vec<model::ArtistRef> {
    let plain = |name: SharedString| model::ArtistRef {
        name,
        source_id: None,
        spotify_uri: None,
    };
    match track {
        Some(track) if !track.artists.is_empty() => track.artists.clone(),
        Some(track) => vec![plain(track.artist.clone())],
        None => vec![plain(SharedString::default())],
    }
}

impl PlayerBar {
    pub(super) fn new(cx: &mut App) -> Self {
        let player = services::AppServices::player(cx);
        let playing = player.read(cx).playing();
        Self {
            player,
            image_cache: services::AppServices::image_cache(cx),
            queue_open: false,
            mascot_playing: playing,
            mascot_transition_generation: 0,
            mascot_transition_from: f32::from(playing),
            mascot_transition_duration: mascot_transition_duration(playing, 0.),
            mascot_reveal: Rc::new(Cell::new(f32::from(playing))),
            scrubber: scrubber::Scrubber::new(cx),
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

    fn queue_button(
        &self,
        palette: CadencePalette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::base::Button {
        gpui_kit::base::Button::new("queue-toggle")
            .key_context("QueueTrigger")
            .accessibility_label("Queue")
            .aria_expanded(self.queue_open)
            .size_10()
            .flex_none()
            .rounded_full()
            .line_height(window.text_style().line_height)
            .text_color(rgb(palette.text_primary))
            .hover(|style| style.bg(rgb(palette.control)))
            .active(|style| style.bg(rgb(palette.control_hover)))
            .focus_visible(|style| style.border_2().border_color(rgb(palette.focus_ring)))
            .when(self.queue_open, |button| button.bg(rgb(palette.selection)))
            .child(components::icon(
                CadenceIcon::Queue,
                tokens::CONTROL_ICON,
                palette.text_primary,
            ))
            .on_click(cx.listener(|this, _, _, cx| {
                this.set_queue_open(!this.queue_open, cx);
                cx.emit(ToggleQueue);
            }))
    }

    fn navigate_on_click(
        event: page::PageEvent,
        cx: &mut Context<Self>,
    ) -> impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |_, _, _, cx| cx.emit(event.clone()))
    }

    /// The artwork opens the album for the pointer only. The title beside it
    /// reaches the same page from the keyboard, and a focus ring drawn under
    /// the art would never show.
    fn artwork_link(
        &self,
        palette: CadencePalette,
        track: Option<&model::Track>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(track) = track else {
            return div()
                .size(tokens::PLAYER_ARTWORK.size)
                .rounded(tokens::PLAYER_ARTWORK.radius)
                .bg(rgb(palette.surface_raised))
                .border_1()
                .border_color(palette.media_border)
                .into_any_element();
        };
        let artwork = components::artwork(
            palette,
            &self.image_cache,
            track.artwork_url.as_deref(),
            tokens::PLAYER_ARTWORK,
            CadenceIcon::MusicNote,
        );
        match playing_album(Some(track)) {
            Some(album) => gpui_kit::base::Button::new("player-artwork")
                .role(gpui_kit::Role::Link)
                .tab_stop(false)
                .accessibility_label(album.name.clone())
                .flex_none()
                .cursor_pointer()
                .on_click(Self::navigate_on_click(
                    page::PageEvent::OpenAlbum(album),
                    cx,
                ))
                .child(artwork)
                .into_any_element(),
            None => artwork,
        }
    }

    fn title_line(
        palette: CadencePalette,
        track: Option<&model::Track>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let title = track.map_or_else(
            || SharedString::new_static("Nothing playing"),
            |track| track.title.clone(),
        );
        let text = div()
            .min_w_0()
            .truncate()
            .text_sm()
            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
            .text_color(rgb(palette.text_primary))
            .child(title);
        div().flex().min_w_0().child(match playing_album(track) {
            Some(album) => components::link(palette, "player-title", window)
                .min_w_0()
                .on_click(Self::navigate_on_click(
                    page::PageEvent::OpenAlbum(album),
                    cx,
                ))
                .child(text)
                .into_any_element(),
            None => text.into_any_element(),
        })
    }

    fn credits_line(
        palette: CadencePalette,
        track: Option<&model::Track>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut credits: Vec<AnyElement> = Vec::new();
        for (index, artist) in artist_credits(track).into_iter().enumerate() {
            if index > 0 {
                credits.push(div().flex_none().child(", ").into_any_element());
            }
            credits.push(Self::artist_link(palette, index, artist, window, cx));
        }
        div()
            .flex()
            .min_w_0()
            .text_xs()
            .text_color(rgb(palette.text_muted))
            .children(credits)
    }

    fn artist_link(
        palette: CadencePalette,
        index: usize,
        artist: model::ArtistRef,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = div().min_w_0().truncate().child(artist.name.clone());
        if artist.source_id.is_none() {
            return name.into_any_element();
        }
        components::link(palette, ("player-artist", index), window)
            .min_w_0()
            .on_click(Self::navigate_on_click(
                page::PageEvent::OpenArtist(artist),
                cx,
            ))
            .child(name)
            .into_any_element()
    }

    fn bar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::Appearance::palette(cx);
        let viewport_width = window.viewport_size().width;
        let rem_size = window.rem_size();
        let compact = uses_compact_player_layout(viewport_width, rem_size);
        let progress_slider_width = if compact {
            compact_progress_slider_width(viewport_width, rem_size)
        } else {
            PROGRESS_SLIDER_WIDTH
        };
        let player = self.player.read(cx);
        let now_playing = player.now_playing().cloned();
        let playing = player.playing();
        let loading = player.loading();
        let position_ms = player.position_ms(cx.background_executor().now());
        let volume = player.volume();
        let volume_thumb_left = (VOLUME_SLIDER_WIDTH - VOLUME_THUMB_SIZE) * volume;
        let volume_thumb_top = (VOLUME_TRACK_HEIGHT - VOLUME_THUMB_SIZE) / 2.;
        let live_track = now_playing.is_some();
        let mascot = services::AppServices::preferences(cx).mascot;
        let show_mascot = live_track && mascot != MascotPreference::None;
        if !show_mascot {
            let target = f32::from(playing);
            self.mascot_playing = playing;
            self.mascot_transition_from = target;
            self.mascot_transition_duration = mascot_transition_duration(playing, 0.);
            self.mascot_reveal.set(target);
        } else if self.mascot_playing != playing {
            self.mascot_transition_from = self.mascot_reveal.get();
            let target = f32::from(playing);
            let remaining = (target - self.mascot_transition_from).abs();
            self.mascot_transition_duration = mascot_transition_duration(playing, remaining);
            self.mascot_playing = playing;
            self.mascot_transition_generation = self.mascot_transition_generation.wrapping_add(1);
        }
        let render_mascot = show_mascot && (playing || self.mascot_reveal.get() > 0.);
        let duration = SharedString::from(now_playing.as_ref().map_or_else(
            || "0:00".to_owned(),
            |track| format_duration(track.duration_ms),
        ));
        let volume_icon = if volume == 0. {
            CadenceIcon::SpeakerMuted
        } else {
            CadenceIcon::Speaker
        };
        let duration_ms = now_playing.as_ref().map_or(0, |track| track.duration_ms);
        div()
            .relative()
            .h_24()
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .gap_6()
            .px_6()
            .bg(rgb(palette.surface))
            .border_t_1()
            .border_color(rgb(palette.border))
            .when(render_mascot, |bar| {
                let from = self.mascot_transition_from;
                let target = f32::from(playing);
                let reveal = self.mascot_reveal.clone();
                let generation = self.mascot_transition_generation as usize;
                let animation = Animation::new(self.mascot_transition_duration)
                    .with_easing(ease_out_quint())
                    .with_max_fps(60.);
                bar.child(player_mascot::render(position_ms, mascot).with_animation(
                    ("mascot-reveal", generation),
                    animation,
                    move |mascot, delta| {
                        let progress = from + (target - from) * delta;
                        reveal.set(progress);
                        let bottom = player_mascot::HIDDEN_BOTTOM
                            + (player_mascot::VISIBLE_BOTTOM - player_mascot::HIDDEN_BOTTOM)
                                * progress;
                        mascot.bottom(px(bottom))
                    },
                ))
            })
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .bg(rgb(palette.surface))
                    .border_t_1()
                    .border_color(rgb(palette.border)),
            )
            .child(
                div()
                    .w(if compact {
                        COMPACT_PLAYER_LEFT_WIDTH
                    } else {
                        PLAYER_LEFT_WIDTH
                    })
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.artwork_link(palette, now_playing.as_ref(), cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Self::title_line(palette, now_playing.as_ref(), window, cx))
                            .child(Self::credits_line(
                                palette,
                                now_playing.as_ref(),
                                window,
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .w(if compact {
                        progress_slider_width + PROGRESS_TIME_WIDTH * 2. + PROGRESS_GAP * 2.
                    } else {
                        PLAYER_CENTER_WIDTH
                    })
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                components::icon_button(palette, "previous", CadenceIcon::SkipBack)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.player.update(cx, |player, cx| player.previous(cx));
                                    })),
                            )
                            .child(
                                components::button(palette, "play-toggle")
                                    .size_10()
                                    .rounded_full()
                                    .bg(rgb(palette.text_primary))
                                    .child(if loading {
                                        Spinner::new()
                                            .color(rgb(palette.on_accent).into())
                                            .into_any_element()
                                    } else {
                                        components::icon(
                                            if playing {
                                                CadenceIcon::Pause
                                            } else {
                                                CadenceIcon::Play
                                            },
                                            tokens::PLAYBACK_ICON,
                                            palette.on_accent,
                                        )
                                        .into_any_element()
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.player.update(cx, |player, cx| player.toggle(cx));
                                    })),
                            )
                            .child(
                                components::icon_button(palette, "next", CadenceIcon::SkipForward)
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
                            .gap(PROGRESS_GAP)
                            .text_size(tokens::CAPTION_TEXT)
                            .text_color(rgb(palette.text_muted))
                            .child(div().w(PROGRESS_TIME_WIDTH).flex_none().text_right().child(
                                format_duration(
                                    self.scrubber.displayed_ms(position_ms, duration_ms),
                                ),
                            ))
                            .child(self.seek_bar(
                                palette,
                                position_ms,
                                duration_ms,
                                progress_slider_width,
                                window,
                                cx,
                            ))
                            .child(div().w(PROGRESS_TIME_WIDTH).flex_none().child(duration)),
                    ),
            )
            .child(
                div()
                    .w(if compact {
                        COMPACT_PLAYER_RIGHT_WIDTH
                    } else {
                        PLAYER_RIGHT_WIDTH
                    })
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .child(self.queue_button(palette, window, cx))
                    .child(
                        components::icon_button(palette, "volume", volume_icon)
                            .test_support()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.player.update(cx, |player, cx| player.toggle_mute(cx));
                            })),
                    )
                    .when(!compact, |controls| {
                        controls.child(
                            div()
                                .id("volume-slider")
                                .w(VOLUME_SLIDER_WIDTH)
                                .h_6()
                                .flex()
                                .items_center()
                                .on_mouse_down(
                                    gpui_kit::MouseButton::Left,
                                    cx.listener(
                                        |this, event: &gpui_kit::MouseDownEvent, window, cx| {
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
                                        .h(VOLUME_TRACK_HEIGHT)
                                        .rounded_full()
                                        .bg(rgb(palette.surface_raised))
                                        .child(
                                            div()
                                                .h_full()
                                                .w(VOLUME_SLIDER_WIDTH * volume)
                                                .rounded_full()
                                                .bg(rgb(palette.text_primary)),
                                        )
                                        .child(
                                            div()
                                                .absolute()
                                                .left(volume_thumb_left)
                                                .top(volume_thumb_top)
                                                .size(VOLUME_THUMB_SIZE)
                                                .rounded_full()
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
            .w(tokens::QUEUE_DRAWER_WIDTH)
            .p_6()
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
                    .mb_6()
                    .child(
                        div()
                            .text_size(tokens::DRAWER_TITLE_TEXT)
                            .font_weight(gpui_kit::FontWeight::MEDIUM)
                            .text_color(rgb(palette.text_primary))
                            .child("Queue"),
                    )
                    .child(
                        components::icon_button(palette, "close-queue", CadenceIcon::Close)
                            .test_support()
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
            .child(div().h_6())
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
            .h(if current {
                tokens::QUEUE_CURRENT_ROW_HEIGHT
            } else {
                tokens::QUEUE_ROW_HEIGHT
            })
            .flex_none()
            .mt_2()
            .px_2p5()
            .rounded_2xl()
            .bg(if current {
                rgb(palette.selection)
            } else {
                rgb(palette.surface)
            })
            .when(!current, |row| {
                row.hover(|style| style.bg(rgb(palette.surface_hover)))
            })
            .flex()
            .items_center()
            .gap_3()
            .child(components::artwork(
                palette,
                &self.image_cache,
                track.artwork_url.as_deref(),
                if current {
                    tokens::QUEUE_CURRENT_ARTWORK
                } else {
                    tokens::TRACK_ARTWORK
                },
                CadenceIcon::MusicNote,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .text_size(tokens::BODY_TEXT)
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .text_color(rgb(palette.text_primary))
                            .child(track.title.clone()),
                    )
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .text_xs()
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
                    .w_11()
                    .flex_none()
                    .text_right()
                    .text_xs()
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

impl PlayerBar {
    /// Abandons an in-flight scrub. The seek commits on release, so a cancelled
    /// gesture never moved playback and has nothing to undo. Reports whether
    /// there was one, so Escape does not also close an overlay behind it.
    pub(super) fn cancel_scrub(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.scrubber.cancel() {
            return false;
        }
        cx.notify();
        true
    }

    pub(super) fn seek_by(&mut self, delta_ms: i64, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        let current = i64::from(self.player.read(cx).position_ms(now));
        let target = current.saturating_add(delta_ms).max(0);
        self.seek_to(u32::try_from(target).unwrap_or(u32::MAX), cx);
    }

    fn seek_to(&mut self, position_ms: u32, cx: &mut Context<Self>) {
        let position_ms = match self.playing_duration_ms(cx) {
            Some(duration_ms) => position_ms.min(duration_ms),
            None => position_ms,
        };
        self.player
            .update(cx, |player, cx| player.seek(position_ms, cx));
        cx.notify();
    }

    /// The duration of the live track, when there is one to seek within.
    fn playing_duration_ms(&self, cx: &App) -> Option<u32> {
        self.player
            .read(cx)
            .now_playing()
            .map(|track| track.duration_ms)
            .filter(|duration| *duration > 0)
    }

    pub(super) fn seek_bar(
        &mut self,
        palette: CadencePalette,
        position_ms: u32,
        duration_ms: u32,
        width: Rems,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // A press whose release never arrives, because the track ended or the
        // window lost it, must not leave the bar stuck on its preview.
        let stale = self.scrubber.gesture_track().is_some_and(|track| {
            self.player
                .read(cx)
                .now_playing()
                .map(|playing| playing.source_id.as_str())
                != Some(track)
        });
        if stale {
            self.scrubber.cancel();
        }

        let focused = self.scrubber.focus_handle.is_focused(window);
        // Hover alone collapses mid-drag: gpui stops reporting hover once a drag
        // is active, but the control is still in use.
        let active = self.scrubber.hovered || self.scrubber.dragging();
        let seekable = duration_ms > 0;

        let grow = gpui_kit::base::transition(
            "scrubber-grow",
            f32::from(active && seekable),
            gpui_kit::base::Transition::new(GROW_DURATION),
            window,
            cx,
        );
        let track_height = TRACK_HEIGHT + (TRACK_HEIGHT_ACTIVE - TRACK_HEIGHT) * grow;
        let thumb_size = THUMB_SIZE * grow;
        let thumb_visible = thumb_size.to_pixels(window.rem_size()) > THUMB_VISIBLE_SIZE;
        let hit_top = (TRACK_HEIGHT.to_pixels(window.rem_size()) - HIT_HEIGHT) / 2.;

        let shown_ms = self.scrubber.displayed_ms(position_ms, duration_ms);
        let fraction = if seekable {
            (shown_ms as f32 / duration_ms as f32).clamp(0., 1.)
        } else {
            0.
        };
        let track_bounds = self.scrubber.track_bounds.clone();

        // The row reserves only the resting track height, so neither the larger
        // target nor the growth on hover can reflow the bar.
        div().relative().w(width).h(TRACK_HEIGHT).flex_none().child(
            div()
                .id("progress-slider")
                .test_support()
                .track_focus(&self.scrubber.focus_handle)
                .key_context("Scrubber")
                .role(gpui_kit::Role::Slider)
                .aria_label("Seek")
                .aria_orientation(gpui_kit::Orientation::Horizontal)
                .aria_min_numeric_value(0.)
                .aria_max_numeric_value(f64::from(duration_ms) / 1000.)
                .aria_numeric_value(f64::from(shown_ms) / 1000.)
                .aria_value(format!(
                    "{} of {}",
                    format_duration(shown_ms),
                    format_duration(duration_ms)
                ))
                .on_action(cx.listener(|this, _: &SeekBackward, _, cx| {
                    this.seek_by(-i64::from(ARROW_STEP_MS), cx);
                }))
                .on_action(cx.listener(|this, _: &SeekForward, _, cx| {
                    this.seek_by(i64::from(ARROW_STEP_MS), cx);
                }))
                .on_action(cx.listener(|this, _: &SeekBackwardLarge, _, cx| {
                    this.seek_by(-i64::from(PAGE_STEP_MS), cx);
                }))
                .on_action(cx.listener(|this, _: &SeekForwardLarge, _, cx| {
                    this.seek_by(i64::from(PAGE_STEP_MS), cx);
                }))
                .on_action(cx.listener(|this, _: &SeekToStart, _, cx| this.seek_to(0, cx)))
                .on_action(cx.listener(|this, _: &SeekToEnd, _, cx| {
                    if let Some(duration_ms) = this.playing_duration_ms(cx) {
                        this.seek_to(duration_ms, cx);
                    }
                }))
                .on_a11y_action(gpui_kit::AccessibleAction::Increment, {
                    let handle = cx.entity().downgrade();
                    move |_, _, cx| {
                        handle
                            .update(cx, |this, cx| this.seek_by(i64::from(ARROW_STEP_MS), cx))
                            .ok();
                    }
                })
                .on_a11y_action(gpui_kit::AccessibleAction::Decrement, {
                    let handle = cx.entity().downgrade();
                    move |_, _, cx| {
                        handle
                            .update(cx, |this, cx| this.seek_by(-i64::from(ARROW_STEP_MS), cx))
                            .ok();
                    }
                })
                .tab_stop(true)
                // Lifted out of the row's flow: the target is wider than the track
                // it wraps, and letting it size the row would push the transport up.
                .absolute()
                .left_0()
                .top(hit_top)
                .w(width)
                .h(HIT_HEIGHT)
                .flex()
                .items_center()
                // gpui raises this only when hover actually changes.
                .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                    this.scrubber.hovered = *hovered;
                    cx.notify();
                }))
                .when(seekable, |scrubber| {
                    scrubber
                        .on_drag(scrubber::ScrubberDrag, |_, _, _, cx| {
                            cx.new(|_| scrubber::NoDragPreview)
                        })
                        .on_mouse_down(
                            gpui_kit::MouseButton::Left,
                            cx.listener(
                                move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                                    // The click that activates a background window
                                    // should not also move playback. gpui accepts the
                                    // first mouse for the whole window, so the control
                                    // has to decline it itself.
                                    if event.first_mouse {
                                        return;
                                    }
                                    window.focus(&this.scrubber.focus_handle, cx);
                                    let Some(track) = this
                                        .player
                                        .read(cx)
                                        .now_playing()
                                        .map(|track| track.source_id.clone())
                                    else {
                                        return;
                                    };
                                    this.scrubber.begin(event.position.x, track, duration_ms);
                                    cx.notify();
                                },
                            ),
                        )
                        .on_drag_move(cx.listener(
                            move |this,
                                  event: &gpui_kit::DragMoveEvent<scrubber::ScrubberDrag>,
                                  _,
                                  cx| {
                                this.scrubber.drag_to(event.event.position.x);
                                cx.notify();
                            },
                        ))
                })
                // Outside `when(seekable)`: a frame that cannot start a gesture must
                // still be able to end one that a previous frame started.
                .on_mouse_up(
                    gpui_kit::MouseButton::Left,
                    cx.listener(|this, _: &gpui_kit::MouseUpEvent, _, cx| {
                        this.commit_scrub(cx);
                    }),
                )
                .on_mouse_up_out(
                    gpui_kit::MouseButton::Left,
                    cx.listener(|this, _: &gpui_kit::MouseUpEvent, _, cx| {
                        this.commit_scrub(cx);
                    }),
                )
                .child(
                    div()
                        .relative()
                        .w_full()
                        .h(track_height)
                        .rounded_full()
                        .bg(rgb(palette.surface_raised))
                        .on_prepaint(move |bounds, _, _| track_bounds.set(bounds))
                        .child(
                            div()
                                .w(relative(fraction))
                                .h_full()
                                .rounded_full()
                                .bg(rgb(palette.text_primary)),
                        )
                        .when(thumb_visible, |track| {
                            track.child(
                                div()
                                    .absolute()
                                    .left(relative(fraction))
                                    .ml(-(thumb_size / 2.))
                                    .top((track_height - thumb_size) / 2.)
                                    .size(thumb_size)
                                    .rounded_full()
                                    .bg(rgb(palette.text_primary)),
                            )
                        }),
                )
                .border_1()
                .border_color(if focused {
                    rgb(palette.focus_ring)
                } else {
                    gpui_kit::transparent_black().into()
                }),
        )
    }

    fn commit_scrub(&mut self, cx: &mut Context<Self>) {
        let track = self
            .player
            .read(cx)
            .now_playing()
            .map(|track| track.source_id.clone());
        let Some(target) = self.scrubber.release(track.as_deref()) else {
            return;
        };
        self.seek_to(target, cx);
    }
}

#[cfg(test)]
mod mascot_tests {
    use super::*;

    #[test]
    fn mascot_transition_duration_is_never_zero() {
        assert_eq!(
            mascot_transition_duration(true, 0.),
            Duration::from_millis(MASCOT_MIN_TRANSITION_MS as u64)
        );
        assert_eq!(
            mascot_transition_duration(false, 0.),
            Duration::from_millis(MASCOT_MIN_TRANSITION_MS as u64)
        );
    }
}
