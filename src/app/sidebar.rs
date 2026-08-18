use super::*;

/// Navigation the sidebar asks the workspace to perform.
pub(super) enum SidebarEvent {
    Navigate(Route),
    OpenPlaylist {
        playlist: model::Playlist,
        origin: Route,
    },
}

/// The library navigation rail.
pub(super) struct Sidebar {
    library: Entity<library::Library>,
    brand_mark: Arc<gpui::Image>,
    /// The route to highlight, pushed by the workspace when it navigates.
    route: Route,
    /// Where a pinned playlist should return to when the listener backs out.
    pinned_origin: Route,
    compact_layout: bool,
    collapsed: bool,
    transition_generation: u64,
    visual_width: Rc<Cell<f32>>,
    transition_from: f32,
    transition_duration: Duration,
}

impl EventEmitter<SidebarEvent> for Sidebar {}

impl Sidebar {
    pub(super) fn new(collapsed: bool, cx: &mut App) -> Self {
        let width = if collapsed { 72. } else { 232. };
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

    /// Tells the rail which route is showing, so it can highlight it.
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
        let expanded_width = if self.compact_layout { 200. } else { 232. };
        let target_width = if collapsed { 72. } else { expanded_width };
        self.transition_from = current_width;
        self.transition_duration =
            sidebar_transition_duration(current_width, target_width, expanded_width);
        self.collapsed = collapsed;
        self.transition_generation = self.transition_generation.wrapping_add(1);
        if let Some(Err(error)) = services::AppServices::set_sidebar_collapsed(collapsed, cx) {
            log::error!("could not save sidebar preference: {error}");
        }
        cx.notify();
    }

    fn panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::Appearance::palette(cx);
        let route = self.route;
        let collapsed = self.collapsed;
        let pinned_origin = self.pinned_origin;
        let expanded_width = if self.compact_layout { 200. } else { 232. };
        let target_width = if collapsed { 72. } else { expanded_width };
        let start_width = self.transition_from;
        let animation_id = self.transition_generation as usize;
        let animation_duration = self.transition_duration;
        let visual_width = self.visual_width.clone();
        let width_range = expanded_width - 72.;
        let start_progress = ((start_width - 72.) / width_range).clamp(0., 1.);
        let target_progress = if collapsed { 0. } else { 1. };
        let label_animation = Animation::new(animation_duration).with_easing(ease_out_quint());
        let nav_item = |id: &'static str,
                        label: &'static str,
                        icon: &'static str,
                        selected_icon: &'static str,
                        target: Route,
                        cx: &mut Context<Self>| {
            let selected =
                route == target || (target == Route::Playlists && route == Route::Playlist);
            components::button(palette, id)
                .w_full()
                .h(px(42.))
                .px(px(12.))
                .justify_start()
                .gap(px(12.))
                .rounded(px(12.))
                .bg(if selected {
                    rgb(palette.selection)
                } else {
                    rgb(palette.canvas)
                })
                .text_color(rgb(if selected {
                    palette.text_primary
                } else {
                    palette.text
                }))
                .text_size(px(14.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .hover(|style| style.bg(rgb(palette.surface_raised)))
                .child(
                    div().w(px(20.)).flex().items_center().child(
                        components::icon(
                            if selected { selected_icon } else { icon },
                            17.,
                            palette.text_primary,
                        )
                        .weight(SymbolWeight::Semibold),
                    ),
                )
                .child(div().whitespace_nowrap().child(label).with_animation(
                    (id, animation_id),
                    label_animation.clone(),
                    move |label, delta| {
                        label.opacity(start_progress + (target_progress - start_progress) * delta)
                    },
                ))
                .on_click(cx.listener(move |_, _, _, cx| cx.emit(SidebarEvent::Navigate(target))))
        };
        let mut pinned_section = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .px(px(10.))
            .child(components::section_label(palette, "Pinned Playlists"));
        let pinned_playlists = self.library.read(cx).pinned_playlists().clone();
        let show_pinned = if self.library.read(cx).local_loaded() {
            for (index, playlist) in pinned_playlists.iter().cloned().enumerate() {
                let selected_playlist = playlist.clone();
                pinned_section = pinned_section.child(
                    components::button(palette, ("pinned-playlist", index))
                        .h(px(32.))
                        .justify_start()
                        .text_size(px(14.))
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
        let brand = components::button(palette, "sidebar-toggle")
            .h(px(48.))
            .w_full()
            .flex_none()
            .justify_start()
            .items_center()
            .gap(px(16.5))
            .px(px(14.))
            .rounded(px(12.))
            .hover(|style| style.bg(rgb(palette.control)))
            .text_color(rgb(palette.text_primary))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .child(img(self.brand_mark.clone()).size(px(32.)).flex_none())
            .child(div().whitespace_nowrap().child("Cadence").with_animation(
                ("sidebar-brand-label", animation_id),
                label_animation.clone(),
                move |label, delta| {
                    label.opacity(start_progress + (target_progress - start_progress) * delta)
                },
            ))
            .child(div().flex_1())
            .child(
                div()
                    .w(px(17.))
                    .h(px(48.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(components::icon("chevron.left", 17., palette.text_primary))
                    .with_animation(
                        ("sidebar-chevron", animation_id),
                        label_animation.clone(),
                        move |button, delta| {
                            button.opacity(
                                start_progress + (target_progress - start_progress) * delta,
                            )
                        },
                    ),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                let collapsed = !this.collapsed;
                this.set_collapsed(collapsed, cx);
            }));

        div()
            .w(px(target_width))
            .h_full()
            .flex_none()
            .overflow_hidden()
            .bg(rgb(palette.canvas))
            .border_r_1()
            .border_color(rgb(palette.border))
            .child(
                div()
                    .w(px(expanded_width))
                    .h_full()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(28.))
                    .p(px(16.))
                    .pt(px(52.))
                    .child(brand)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .px(px(12.))
                                    .pb(px(4.))
                                    .child(components::section_label(palette, "Library"))
                                    .with_animation(
                                        ("sidebar-library-label", animation_id),
                                        label_animation.clone(),
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
                                "Liked Songs",
                                "heart",
                                "heart.fill",
                                Route::LikedSongs,
                                cx,
                            ))
                            .child(nav_item(
                                "nav-favorites",
                                "Favorites",
                                "star",
                                "star.fill",
                                Route::Favorites,
                                cx,
                            ))
                            .child(nav_item(
                                "nav-playlist",
                                "Playlists",
                                "music.note.list",
                                "music.note.list",
                                Route::Playlists,
                                cx,
                            ))
                            .child(nav_item(
                                "nav-recent",
                                "Recently played",
                                "clock",
                                "clock.fill",
                                Route::Recent,
                                cx,
                            )),
                    )
                    .when(show_pinned && !collapsed, |sidebar| {
                        sidebar.child(div().child(pinned_section).with_animation(
                            ("sidebar-pinned", animation_id),
                            label_animation.clone(),
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
                    sidebar.w(px(width))
                },
            )
    }
}

impl Render for Sidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.panel(cx)
    }
}
