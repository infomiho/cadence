use super::*;

impl CadenceApp {
    pub(super) fn sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let route = self.route;
        let collapsed = self.sidebar_collapsed;
        let pinned_origin = if route == Route::Playlist {
            self.playlist_origin
        } else {
            route
        };
        let expanded_width = if self.compact_layout { 200. } else { 232. };
        let target_width = if collapsed { 72. } else { expanded_width };
        let start_width = self.sidebar_transition_from;
        let animation_id = self.sidebar_transition_generation as usize;
        let animation_duration = self.sidebar_transition_duration;
        let visual_width = self.sidebar_visual_width.clone();
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
            components::button(self.palette, id)
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
                .on_click(cx.listener(move |this, _, _, cx| this.navigate(target, cx)))
        };
        let mut pinned_section = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .px(px(10.))
            .child(components::section_label(self.palette, "Pinned Playlists"));
        let show_pinned = if self.local_state_loaded {
            for (index, playlist) in self.pinned_playlists.iter().cloned().enumerate() {
                let selected_playlist = playlist.clone();
                pinned_section = pinned_section.child(
                    components::button(self.palette, ("pinned-playlist", index))
                        .h(px(32.))
                        .justify_start()
                        .text_size(px(14.))
                        .text_color(rgb(palette.text))
                        .child(playlist.name)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.selected_spotify_playlist = Some(selected_playlist.clone());
                            this.playlist_tracks = Arc::default();
                            this.playlist_loaded = false;
                            this.playlist_error = None;
                            this.load_playlist(selected_playlist.clone());
                            this.open_playlist(pinned_origin, cx);
                        })),
                );
            }
            !self.pinned_playlists.is_empty()
        } else {
            false
        };
        let brand = components::button(self.palette, "sidebar-toggle")
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
                this.set_sidebar_collapsed(!this.sidebar_collapsed, cx);
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
                                    .child(components::section_label(self.palette, "Library"))
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

    pub(super) fn toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let route = self.route;
        let profile_name = self.profile.as_ref().map_or_else(
            || "Spotify account".to_owned(),
            |profile| profile.display_name.clone(),
        );
        let profile_initials = components::initials(&profile_name);
        let profile_artwork = self
            .profile
            .as_ref()
            .and_then(|profile| profile.artwork_url.as_deref());
        let account_menu_was_open = self.account_menu_open;
        let playback_error = self.player.read(cx).error().cloned();
        let account_detail: SharedString = if let Some(error) = &playback_error {
            error.clone().into()
        } else if let Some(error) = &self.last_error {
            error.clone().into()
        } else {
            match &self.connection_state {
                ConnectionState::Starting => "Starting Spotify…".into(),
                ConnectionState::Failed => "Backend unavailable".into(),
                ConnectionState::SetupRequired => "Developer app required".into(),
                ConnectionState::AuthorizationRequired => "Not connected".into(),
                ConnectionState::Connecting => "Connecting…".into(),
                ConnectionState::Ready => "Spotify connected".into(),
            }
        };
        let can_connect = matches!(
            self.connection_state,
            ConnectionState::AuthorizationRequired
        );
        let detail_origin = match self.route {
            Route::Playlist => Some(self.playlist_origin),
            Route::Artist => Some(self.artist_origin),
            Route::Album => Some(self.album_origin),
            Route::Settings => Some(self.settings_origin),
            _ => None,
        };
        let search = div()
            .id("search-field")
            .w(px(if self.compact_layout { 340. } else { 520. }))
            .h(px(40.))
            .flex()
            .items_center()
            .justify_start()
            .gap(px(10.))
            .px(px(14.))
            .rounded(px(12.))
            .border_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.surface))
            .text_size(px(14.))
            .text_color(rgb(palette.text_muted))
            .child(components::icon("magnifyingglass", 16., palette.text_muted))
            .child(
                Input::new(&self.search_input)
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .flex_1()
                    .min_w_0()
                    .h_full(),
            )
            .child(
                div()
                    .px(px(6.))
                    .py(px(2.))
                    .rounded(px(6.))
                    .bg(rgb(palette.surface_raised))
                    .text_color(rgb(palette.text))
                    .text_size(px(11.))
                    .child("⌘ K"),
            );

        div()
            .h(px(72.))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.))
            .px(px(28.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .when_some(detail_origin, |group, origin| {
                        group.child(
                            components::icon_button(self.palette, "detail-back", "chevron.left")
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.navigate(origin, cx)),
                                ),
                        )
                    })
                    .when(route == Route::Settings, |group| {
                        group.child(
                            div()
                                .h(px(40.))
                                .flex()
                                .items_center()
                                .text_size(px(18.))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(palette.text_primary))
                                .child("Settings"),
                        )
                    })
                    .when(route != Route::Settings, |group| group.child(search)),
            )
            .child(
                div()
                    .relative()
                    .child(
                        components::button(self.palette, "account")
                            .size(px(40.))
                            .rounded(px(40.))
                            .overflow_hidden()
                            .text_color(rgb(palette.on_accent))
                            .text_size(px(12.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(components::profile_avatar(
                                profile_artwork,
                                profile_initials,
                            ))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.account_menu_open = !account_menu_was_open;
                                if this.account_menu_open {
                                    this.close_queue(cx);
                                    this.track_menu_open = None;
                                }
                                cx.notify();
                            })),
                    )
                    .when(self.account_menu_open, |anchor| {
                        anchor.child(deferred(
                            components::menu_surface(self.palette)
                                .on_mouse_up_out(
                                    gpui::MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        this.account_menu_open = false;
                                        cx.notify();
                                    }),
                                )
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|_, _, _, cx| cx.stop_propagation()),
                                )
                                .absolute()
                                .top(px(48.))
                                .right_0()
                                .child(
                                    div()
                                        .px(px(10.))
                                        .py(px(8.))
                                        .child(
                                            div()
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .text_color(rgb(palette.text_primary))
                                                .child(profile_name),
                                        )
                                        .child(
                                            div()
                                                .mt(px(2.))
                                                .text_size(px(12.))
                                                .text_color(rgb(palette.text_muted))
                                                .child(account_detail),
                                        ),
                                )
                                .when(can_connect, |menu| {
                                    menu.child(
                                        components::menu_item(
                                            self.palette,
                                            "account-connect",
                                            "key",
                                            "Log in with Spotify",
                                            false,
                                        )
                                        .on_click(
                                            cx.listener(|this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.account_menu_open = false;
                                                this.connection_state = ConnectionState::Connecting;
                                                this.authenticate();
                                                cx.notify();
                                            }),
                                        ),
                                    )
                                })
                                .child(
                                    div()
                                        .mt(px(4.))
                                        .pt(px(6.))
                                        .border_t_1()
                                        .border_color(rgb(palette.border))
                                        .child(
                                            components::menu_item(
                                                self.palette,
                                                "account-settings",
                                                "gearshape",
                                                "Settings",
                                                false,
                                            )
                                            .on_click(
                                                cx.listener(|this, _, _, cx| {
                                                    cx.stop_propagation();
                                                    this.open_settings(cx);
                                                }),
                                            ),
                                        ),
                                )
                                .when(
                                    matches!(self.connection_state, ConnectionState::Ready),
                                    |menu| {
                                        menu.child(
                                            components::menu_item(
                                                self.palette,
                                                "account-logout",
                                                "rectangle.portrait.and.arrow.right",
                                                "Logout",
                                                true,
                                            )
                                            .on_click(
                                                cx.listener(|this, _, _, cx| {
                                                    cx.stop_propagation();
                                                    this.account_menu_open = false;
                                                    this.logout();
                                                    cx.notify();
                                                }),
                                            ),
                                        )
                                    },
                                ),
                        ))
                    }),
            )
    }

    pub(super) fn page_heading(
        &self,
        title: impl Into<SharedString>,
        detail: impl Into<SharedString>,
    ) -> Div {
        let palette = self.palette;
        div()
            .flex()
            .items_end()
            .justify_between()
            .gap(px(24.))
            .mb(px(24.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(7.))
                    .child(
                        div()
                            .text_size(px(40.))
                            .line_height(px(44.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(palette.text_primary))
                            .child(title.into()),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(rgb(palette.text_muted))
                            .child(detail.into()),
                    ),
            )
    }
}
