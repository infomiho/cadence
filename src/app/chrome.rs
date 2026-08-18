use super::*;

impl CadenceApp {
    pub(super) fn toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let route = self.route;
        let profile = self.session.read(cx).profile().cloned();
        let profile_name = profile.as_ref().map_or_else(
            || "Spotify account".to_owned(),
            |profile| profile.display_name.clone(),
        );
        let profile_initials = components::initials(&profile_name);
        let profile_artwork = profile
            .as_ref()
            .and_then(|profile| profile.artwork_url.as_deref());
        let account_menu_was_open = self.account_menu_open;
        let playback_error = self.player.read(cx).error().cloned();
        let account_detail: SharedString = if let Some(error) = &playback_error {
            error.clone().into()
        } else if let Some(error) = &self.last_error {
            error.clone().into()
        } else {
            match self.session.read(cx).state() {
                ConnectionState::Starting => "Starting Spotify…".into(),
                ConnectionState::Failed => "Backend unavailable".into(),
                ConnectionState::SetupRequired => "Developer app required".into(),
                ConnectionState::AuthorizationRequired => "Not connected".into(),
                ConnectionState::Connecting => "Connecting…".into(),
                ConnectionState::Ready => "Spotify connected".into(),
            }
        };
        let can_connect = matches!(
            self.session.read(cx).state(),
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
                                                this.session.update(cx, |session, cx| {
                                                    session.set_connecting(cx)
                                                });
                                                this.authenticate(cx);
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
                                .when(self.session.read(cx).is_ready(), |menu| {
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
                                                this.logout(cx);
                                                cx.notify();
                                            }),
                                        ),
                                    )
                                }),
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

    pub(super) fn spotify_app_change_confirmation(&self, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let consequence = if self.session.read(cx).profile().is_some() {
            "This signs you out, removes the saved Client ID, and restarts Spotify setup. Your Cadence favorites and settings stay."
        } else {
            "This removes the saved Client ID and restarts Spotify setup. Your Cadence favorites and settings stay."
        };
        div()
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .left_0()
            .occlude()
            .bg(palette.scrim)
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(440.))
                    .p(px(24.))
                    .rounded(px(16.))
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(rgb(palette.surface))
                    .shadow_lg()
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(palette.text_primary))
                            .child("Change Spotify developer app?"),
                    )
                    .child(
                        div()
                            .mt(px(10.))
                            .text_size(px(14.))
                            .line_height(relative(1.5))
                            .text_color(rgb(palette.text))
                            .child(consequence),
                    )
                    .child(
                        div()
                            .mt(px(24.))
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                components::settings_button(
                                    self.palette,
                                    "cancel-spotify-app-change",
                                    "Cancel",
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.cancel_spotify_app_change(cx);
                                    },
                                )),
                            )
                            .child(
                                components::button(self.palette, "confirm-spotify-app-change")
                                    .h(px(40.))
                                    .px(px(14.))
                                    .rounded(px(10.))
                                    .bg(rgb(palette.destructive))
                                    .text_size(px(13.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(palette.on_destructive))
                                    .hover(|style| style.opacity(0.88))
                                    .child("Change developer app")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.confirm_spotify_app_change(cx);
                                    })),
                            ),
                    ),
            )
    }
}
