use super::*;

/// Navigation the sidebar asks the workspace to perform.
pub(super) enum SidebarEvent {
    Navigate(Route),
    Failed(String),
    OpenPlaylist {
        playlist: model::Playlist,
        origin: Route,
    },
}

/// The library navigation rail.
pub(super) struct Sidebar {
    library: Entity<library::Library>,
    brand_mark: Arc<gpui_kit::Image>,
    /// The route to highlight, pushed by the workspace when it navigates.
    route: Route,
    /// Where a pinned playlist should return to when the listener backs out.
    pinned_origin: Route,
    compact_layout: bool,
    collapsed: bool,
    transition_generation: u64,
    visual_width: Rc<Cell<Rems>>,
    transition_from: Rems,
    transition_duration: Duration,
}

impl EventEmitter<SidebarEvent> for Sidebar {}

fn expanded_sidebar_width(compact_layout: bool) -> Rems {
    if compact_layout {
        COMPACT_EXPANDED_SIDEBAR_WIDTH
    } else {
        EXPANDED_SIDEBAR_WIDTH
    }
}

impl Sidebar {
    pub(super) fn new(collapsed: bool, cx: &mut App) -> Self {
        let width = if collapsed {
            COLLAPSED_SIDEBAR_WIDTH
        } else {
            expanded_sidebar_width(false)
        };
        Self {
            library: services::AppServices::library(cx),
            brand_mark: services::AppServices::brand_mark(cx),
            route: Route::LikedSongs,
            pinned_origin: Route::LikedSongs,
            compact_layout: false,
            collapsed,
            transition_generation: 0,
            visual_width: Rc::new(Cell::new(width)),
            transition_from: width,
            transition_duration: Duration::from_millis(1),
        }
    }

    pub(super) fn show_route(
        &mut self,
        route: Route,
        pinned_origin: Route,
        cx: &mut Context<Self>,
    ) {
        if self.route != route || self.pinned_origin != pinned_origin {
            self.route = route;
            self.pinned_origin = pinned_origin;
            cx.notify();
        }
    }

    pub(super) fn set_compact_layout(&mut self, compact: bool, cx: &mut Context<Self>) {
        if self.compact_layout != compact {
            self.compact_layout = compact;
            cx.notify();
        }
    }

    pub(super) fn set_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        if self.collapsed == collapsed {
            return;
        }
        let current_width = self.visual_width.get();
        let expanded_width = expanded_sidebar_width(self.compact_layout);
        let target_width = if collapsed {
            COLLAPSED_SIDEBAR_WIDTH
        } else {
            expanded_width
        };
        self.transition_from = current_width;
        self.transition_duration =
            sidebar_transition_duration(current_width, target_width, expanded_width);
        self.collapsed = collapsed;
        self.transition_generation = self.transition_generation.wrapping_add(1);
        if let Some(Err(error)) = services::AppServices::set_sidebar_collapsed(collapsed, cx) {
            cx.emit(SidebarEvent::Failed(format!(
                "Could not save sidebar preference: {error}"
            )));
        }
        cx.notify();
    }

    fn panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::Appearance::palette(cx);
        let route = self.route;
        let collapsed = self.collapsed;
        let pinned_origin = self.pinned_origin;
        let expanded_width = expanded_sidebar_width(self.compact_layout);
        let target_width = if collapsed {
            COLLAPSED_SIDEBAR_WIDTH
        } else {
            expanded_width
        };
        let start_width = self.transition_from;
        let animation_id = self.transition_generation as usize;
        let animation_duration = self.transition_duration;
        let visual_width = self.visual_width.clone();
        let width_range = expanded_width - COLLAPSED_SIDEBAR_WIDTH;
        let start_progress =
            ((start_width - COLLAPSED_SIDEBAR_WIDTH).0 / width_range.0).clamp(0., 1.);
        let row_width = expanded_width - SIDEBAR_CONTENT_PAD * 2.;
        let target_progress = if collapsed { 0. } else { 1. };
        let row_animation = Animation::new(animation_duration).with_easing(ease_out_quint());
        let nav_item = |id: &'static str,
                        fill_id: &'static str,
                        label: &'static str,
                        icon: CadenceIcon,
                        selected_icon: CadenceIcon,
                        target: Route,
                        cx: &mut Context<Self>| {
            let selected =
                route == target || (target == Route::Playlists && route == Route::Playlist);
            // The pill carries selection and hover, sized to what it visually
            // covers: the icon when collapsed, the whole row when expanded.
            let fill = div()
                .h(tokens::NAV_ROW_HEIGHT)
                .rounded_xl()
                .overflow_hidden()
                .flex()
                .items_center()
                .gap_3()
                .pr(NAV_ROW_PAD)
                .when(selected, |fill| fill.bg(rgb(palette.selection)))
                .hover(|style| style.bg(rgb(palette.surface_raised)))
                .child(
                    div()
                        .w_5()
                        .flex_none()
                        .flex()
                        .items_center()
                        .child(components::icon(
                            if selected { selected_icon } else { icon },
                            tokens::CONTROL_ICON,
                            palette.text_primary,
                        )),
                )
                .child(div().whitespace_nowrap().child(label).with_animation(
                    (id, animation_id),
                    row_animation.clone(),
                    move |label, delta| {
                        label.opacity(start_progress + (target_progress - start_progress) * delta)
                    },
                ))
                .with_animation(
                    (fill_id, animation_id),
                    row_animation.clone(),
                    move |fill, delta| {
                        let progress = start_progress + (target_progress - start_progress) * delta;
                        let (width, left, pad) = sidebar_fill_geometry(
                            NAV_ROW_PAD,
                            NAV_GLYPH_WIDTH,
                            row_width,
                            progress,
                        );
                        fill.w(width).ml(left).pl(pad)
                    },
                );
            components::button(palette, id)
                .w_full()
                .h(tokens::NAV_ROW_HEIGHT)
                .justify_start()
                .text_color(rgb(if selected {
                    palette.text_primary
                } else {
                    palette.text
                }))
                .text_sm()
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .child(fill)
                .on_click(cx.listener(move |_, _, _, cx| cx.emit(SidebarEvent::Navigate(target))))
        };
        let mut pinned_section = div()
            .flex()
            .flex_col()
            .gap_2()
            .px_2p5()
            .child(components::section_label(palette, "Pinned Playlists"));
        let pinned_playlists = self.library.read(cx).pinned_playlists().clone();
        let show_pinned = if self.library.read(cx).local_loaded() {
            for (index, playlist) in pinned_playlists.iter().cloned().enumerate() {
                let selected_playlist = playlist.clone();
                pinned_section = pinned_section.child(
                    components::button(palette, ("pinned-playlist", index))
                        .h_8()
                        .justify_start()
                        .text_sm()
                        .text_color(rgb(palette.text))
                        .child(playlist.name)
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(SidebarEvent::OpenPlaylist {
                                playlist: selected_playlist.clone(),
                                origin: pinned_origin,
                            });
                        })),
                );
            }
            !pinned_playlists.is_empty()
        } else {
            false
        };
        let brand_fill = div()
            .h_12()
            .rounded_xl()
            .overflow_hidden()
            .flex()
            .items_center()
            .gap(tokens::BRAND_LABEL_GAP)
            .pr(BRAND_ROW_PAD)
            .hover(|style| style.bg(rgb(palette.control)))
            .child(
                img(self.brand_mark.clone())
                    .size(BRAND_LOGO_SIZE)
                    .flex_none(),
            )
            .child(div().whitespace_nowrap().child("Cadence").with_animation(
                ("sidebar-brand-label", animation_id),
                row_animation.clone(),
                move |label, delta| {
                    label.opacity(start_progress + (target_progress - start_progress) * delta)
                },
            ))
            .child(div().flex_1())
            .child(
                div()
                    .w(tokens::CONTROL_ICON)
                    .h_12()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(components::icon(
                        CadenceIcon::ChevronLeft,
                        tokens::CONTROL_ICON,
                        palette.text_primary,
                    ))
                    .with_animation(
                        ("sidebar-chevron", animation_id),
                        row_animation.clone(),
                        move |button, delta| {
                            button.opacity(
                                start_progress + (target_progress - start_progress) * delta,
                            )
                        },
                    ),
            )
            .with_animation(
                ("sidebar-brand-fill", animation_id),
                row_animation.clone(),
                move |fill, delta| {
                    let progress = start_progress + (target_progress - start_progress) * delta;
                    let (width, left, pad) =
                        sidebar_fill_geometry(BRAND_ROW_PAD, BRAND_LOGO_SIZE, row_width, progress);
                    fill.w(width).ml(left).pl(pad)
                },
            );
        let brand = components::button(palette, "sidebar-toggle")
            .h_12()
            .w_full()
            .flex_none()
            .justify_start()
            .items_center()
            .text_color(rgb(palette.text_primary))
            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
            .child(brand_fill)
            .on_click(cx.listener(|this, _, _, cx| {
                let collapsed = !this.collapsed;
                this.set_collapsed(collapsed, cx);
            }));

        div()
            .w(target_width)
            .h_full()
            .flex_none()
            .overflow_hidden()
            .bg(rgb(palette.canvas))
            .border_r_1()
            .border_color(rgb(palette.border))
            .child(
                div()
                    .w(expanded_width)
                    .h_full()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap_7()
                    .p(SIDEBAR_CONTENT_PAD)
                    .pt(tokens::SIDEBAR_TOP_INSET)
                    .child(brand)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .px_3()
                                    .pb_1()
                                    .child(components::section_label(palette, "Library"))
                                    .with_animation(
                                        ("sidebar-library-label", animation_id),
                                        row_animation.clone(),
                                        move |label, delta| {
                                            label.opacity(
                                                start_progress
                                                    + (target_progress - start_progress) * delta,
                                            )
                                        },
                                    ),
                            )
                            .child(nav_item(
                                "nav-library",
                                "nav-library-fill",
                                "Liked Songs",
                                CadenceIcon::Heart,
                                CadenceIcon::HeartFilled,
                                Route::LikedSongs,
                                cx,
                            ))
                            .child(nav_item(
                                "nav-favorites",
                                "nav-favorites-fill",
                                "Favorites",
                                CadenceIcon::Star,
                                CadenceIcon::StarFilled,
                                Route::Favorites,
                                cx,
                            ))
                            .child(nav_item(
                                "nav-playlist",
                                "nav-playlist-fill",
                                "Playlists",
                                CadenceIcon::Playlist,
                                CadenceIcon::PlaylistFilled,
                                Route::Playlists,
                                cx,
                            ))
                            .child(nav_item(
                                "nav-recent",
                                "nav-recent-fill",
                                "Recently played",
                                CadenceIcon::Clock,
                                CadenceIcon::ClockFilled,
                                Route::Recent,
                                cx,
                            )),
                    )
                    .when(show_pinned && !collapsed, |sidebar| {
                        sidebar.child(div().child(pinned_section).with_animation(
                            ("sidebar-pinned", animation_id),
                            row_animation.clone(),
                            move |pinned, delta| {
                                pinned.opacity(start_progress + (1. - start_progress) * delta)
                            },
                        ))
                    })
                    .child(div().flex_1()),
            )
            .with_animation(
                ("sidebar-width", animation_id),
                Animation::new(animation_duration).with_easing(ease_out_quint()),
                move |sidebar, delta| {
                    let width = interpolate_sidebar_width(start_width, target_width, delta);
                    visual_width.set(width);
                    sidebar.w(width)
                },
            )
    }
}

impl Render for Sidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.panel(cx)
    }
}
