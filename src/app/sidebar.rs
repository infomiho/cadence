use super::*;
use gpui_kit::TestSupportExt as _;

/// Navigation the sidebar asks the workspace to perform.
pub(super) enum SidebarEvent {
    Navigate(Route),
    Failed(SharedString),
    OpenPlaylist {
        playlist: model::Playlist,
        origin: Route,
    },
}

/// A library route the rail links to.
struct NavItem {
    id: &'static str,
    fill_id: &'static str,
    label: &'static str,
    icon: CadenceIcon,
    selected_icon: CadenceIcon,
    route: Route,
}

const NAV_ITEMS: [NavItem; 4] = [
    NavItem {
        id: "nav-library",
        fill_id: "nav-library-fill",
        label: "Liked Songs",
        icon: CadenceIcon::Heart,
        selected_icon: CadenceIcon::HeartFilled,
        route: Route::LikedSongs,
    },
    NavItem {
        id: "nav-favorites",
        fill_id: "nav-favorites-fill",
        label: "Favorites",
        icon: CadenceIcon::Star,
        selected_icon: CadenceIcon::StarFilled,
        route: Route::Favorites,
    },
    NavItem {
        id: "nav-playlist",
        fill_id: "nav-playlist-fill",
        label: "Playlists",
        icon: CadenceIcon::Playlist,
        selected_icon: CadenceIcon::PlaylistFilled,
        route: Route::Playlists,
    },
    NavItem {
        id: "nav-recent",
        fill_id: "nav-recent-fill",
        label: "Recently played",
        icon: CadenceIcon::Clock,
        selected_icon: CadenceIcon::ClockFilled,
        route: Route::Recent,
    },
];

/// Where the rows stand in the rail's collapse or expand animation.
struct RowTransition {
    generation: usize,
    animation: Animation,
    start_progress: f32,
    target_progress: f32,
    row_width: Rems,
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
    /// The toggle and navigation rows draw keyboard focus on the fill inside
    /// them, so they own their focus handles.
    toggle_focus: FocusHandle,
    /// One per entry of [`NAV_ITEMS`], in the same order.
    nav_focus: [FocusHandle; NAV_ITEMS.len()],
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
            toggle_focus: cx.focus_handle().tab_stop(true),
            nav_focus: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
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
        if let Some(result) = services::AppServices::set_sidebar_collapsed(collapsed, cx)
            && let Some(error) = services::preference_save_error("sidebar", result)
        {
            cx.emit(SidebarEvent::Failed(error));
        }
        cx.notify();
    }

    #[cfg(test)]
    pub(super) fn rendered_width(&self) -> Rems {
        self.visual_width.get()
    }

    fn nav_item(
        &self,
        item: &NavItem,
        focus_handle: &FocusHandle,
        transition: &RowTransition,
        palette: CadencePalette,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let route = self.route;
        let target = item.route;
        let selected = route == target || (target == Route::Playlists && route == Route::Playlist);
        let glyph = if selected {
            item.selected_icon
        } else {
            item.icon
        };
        let focus_visible = components::is_focus_visible(focus_handle, window);
        let start_progress = transition.start_progress;
        let target_progress = transition.target_progress;
        let row_width = transition.row_width;
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
            .when(focus_visible, |fill| {
                fill.shadow(components::inset_focus_ring(palette))
            })
            .hover(|style| style.bg(rgb(palette.surface_raised)))
            .child(
                div()
                    .w_5()
                    .flex_none()
                    .flex()
                    .items_center()
                    .child(components::icon(
                        glyph,
                        tokens::CONTROL_ICON,
                        palette.text_primary,
                    )),
            )
            .child(div().whitespace_nowrap().child(item.label).with_animation(
                (item.id, transition.generation),
                transition.animation.clone(),
                move |label, delta| {
                    label.opacity(start_progress + (target_progress - start_progress) * delta)
                },
            ))
            .with_animation(
                (item.fill_id, transition.generation),
                transition.animation.clone(),
                move |fill, delta| {
                    let progress = start_progress + (target_progress - start_progress) * delta;
                    let (width, left, pad) =
                        sidebar_fill_geometry(NAV_ROW_PAD, NAV_GLYPH_WIDTH, row_width, progress);
                    fill.w(width).ml(left).pl(pad)
                },
            );
        components::bare_button(item.id)
            .test_support()
            .track_focus(focus_handle)
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
            .into_any_element()
    }

    fn panel(&mut self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::Appearance::palette(cx);
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
        let transition = RowTransition {
            generation: animation_id,
            animation: row_animation.clone(),
            start_progress,
            target_progress,
            row_width,
        };
        let nav_rows: Vec<_> = NAV_ITEMS
            .iter()
            .zip(&self.nav_focus)
            .map(|(item, focus_handle)| {
                self.nav_item(item, focus_handle, &transition, palette, window, cx)
            })
            .collect();
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
                    components::flush_button(palette, ("pinned-playlist", index))
                        .h_8()
                        .rounded_lg()
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
        let toggle_focus_visible = components::is_focus_visible(&self.toggle_focus, window);
        let brand_fill = div()
            .h_12()
            .rounded_xl()
            .overflow_hidden()
            .flex()
            .items_center()
            .gap(tokens::BRAND_LABEL_GAP)
            .pr(BRAND_ROW_PAD)
            .when(toggle_focus_visible, |fill| {
                fill.shadow(components::inset_focus_ring(palette))
            })
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
        let brand = components::bare_button("sidebar-toggle")
            .test_support()
            .track_focus(&self.toggle_focus)
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
                            .children(nav_rows),
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.panel(window, cx)
    }
}
