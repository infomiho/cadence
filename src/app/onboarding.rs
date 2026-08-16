use super::*;

pub(super) const SPOTIFY_DASHBOARD_URL: &str = "https://developer.spotify.com/dashboard";
pub(super) const SPOTIFY_REDIRECT_URI: &str = "http://127.0.0.1:8888/callback";

impl CadenceApp {
    pub(super) fn onboarding_page(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let palette = self.palette;
        let compact = self.compact_layout;
        let context_rail = (!compact).then(|| self.onboarding_context_rail(cx));
        let content = match self.connection_state {
            ConnectionState::SetupRequired => self.spotify_setup_form(cx),
            ConnectionState::AuthorizationRequired | ConnectionState::Connecting => {
                self.spotify_login_form(cx)
            }
            ConnectionState::Starting | ConnectionState::Ready => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(Spinner::new().large()),
        };

        div()
            .id("spotify-onboarding")
            .key_context("Cadence")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::dismiss_overlay))
            .size_full()
            .overflow_y_scroll()
            .bg(rgb(palette.surface))
            .font_family("Inter")
            .text_color(rgb(palette.text))
            .child(
                div()
                    .h_full()
                    .min_h(px(640.))
                    .w_full()
                    .flex()
                    .when_some(context_rail, |layout, rail| layout.child(rail))
                    .child(content),
            )
    }

    fn onboarding_context_rail(&self, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let show_configuration = matches!(
            self.connection_state,
            ConnectionState::AuthorizationRequired | ConnectionState::Connecting
        );
        div()
            .w(px(420.))
            .flex_none()
            .p(px(56.))
            .border_r_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.canvas))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(img(self.brand_mark.clone()).size(px(40.)).flex_none())
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(palette.text_primary))
                            .child("Cadence"),
                    ),
            )
            .child(
                div()
                    .mt(px(56.))
                    .text_size(px(30.))
                    .line_height(relative(1.12))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(palette.text_primary))
                    .child("Your Spotify,\nin Cadence"),
            )
            .child(
                div()
                    .mt(px(20.))
                    .text_size(px(14.))
                    .line_height(relative(1.55))
                    .text_color(rgb(palette.text_muted))
                    .child(
                        "Use a Spotify developer app you control. Cadence stores only its public Client ID.",
                    ),
            )
            .when(show_configuration, |rail| {
                rail.child(self.spotify_login_configuration(cx)).child(
                    div()
                        .mt(px(12.))
                        .text_size(px(11.))
                        .text_color(rgb(palette.text_muted))
                        .child("No client secret needed. Tokens stay in Keychain."),
                )
            })
            .child(div().flex_1())
    }

    fn spotify_login_configuration(&self, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let connecting = matches!(self.connection_state, ConnectionState::Connecting);
        let client_id = self.spotify_client_id.clone().unwrap_or_default();
        let dashboard_url = format!("{SPOTIFY_DASHBOARD_URL}/{client_id}");

        div()
            .mt(px(32.))
            .p(px(16.))
            .rounded(px(12.))
            .border_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.surface))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(palette.text_muted))
                    .child("Saved in Cadence"),
            )
            .child(
                div()
                    .mt(px(6.))
                    .text_size(px(13.))
                    .text_color(rgb(palette.text_primary))
                    .child(client_id),
            )
            .when(
                !connecting
                    && self.spotify_client_id_source == Some(ClientIdSource::Saved)
                    && !self.spotify_app_change_confirmation_open,
                |card| {
                    card.child(
                        div().mt(px(14.)).flex().justify_start().child(
                            self.settings_button(
                                "login-change-spotify-app",
                                "Change developer app",
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.request_spotify_app_change(cx);
                            })),
                        ),
                    )
                },
            )
            .when(
                self.spotify_client_id_source == Some(ClientIdSource::Environment),
                |card| {
                    card.child(
                        self.settings_button("login-open-spotify-dashboard", "Open dashboard")
                            .mt(px(14.))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.open_url(&dashboard_url);
                            })),
                    )
                    .child(
                        div()
                            .mt(px(10.))
                            .text_size(px(11.))
                            .line_height(relative(1.4))
                            .text_color(rgb(palette.text_muted))
                            .child("Change SPOTIFY_CLIENT_ID outside Cadence."),
                    )
                },
            )
    }

    fn onboarding_task_header(&self, title: &'static str, detail: &'static str) -> Div {
        let palette = self.palette;
        div()
            .child(
                div()
                    .text_size(px(28.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(palette.text_primary))
                    .child(title),
            )
            .child(
                div()
                    .mt(px(8.))
                    .text_size(px(14.))
                    .text_color(rgb(palette.text_muted))
                    .child(detail),
            )
    }

    fn spotify_setup_form(&mut self, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let error = self.spotify_setup_error.clone();
        let has_error = error.is_some();
        div()
            .flex_1()
            .w_full()
            .max_w(px(720.))
            .mx_auto()
            .min_w_0()
            .p(px(if self.compact_layout { 32. } else { 48. }))
            .flex()
            .flex_col()
            .child(self.onboarding_task_header("Set up Spotify", "Two steps, about two minutes."))
            .child(
                div()
                    .mt(px(28.))
                    .flex()
                    .gap(px(16.))
                    .child(self.onboarding_step_number("1"))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.onboarding_step_title("Create a Spotify app"))
                            .child(self.onboarding_detail(
                                "Select Web API in the Spotify developer dashboard.",
                            ))
                            .child(
                                self.settings_button(
                                    "open-spotify-dashboard",
                                    "Open Spotify dashboard",
                                )
                                    .mt(px(16.))
                                    .w(px(220.))
                                    .on_click(cx.listener(|_, _, _, cx| {
                                        cx.open_url(SPOTIFY_DASHBOARD_URL);
                                    })),
                            )
                            .child(
                                div()
                                    .mt(px(24.))
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(palette.text_primary))
                                    .child("Add this redirect URI"),
                            )
                            .child(
                                div()
                                    .mt(px(8.))
                                    .h(px(48.))
                                    .rounded(px(12.))
                                    .border_1()
                                    .border_color(rgb(palette.border))
                                    .flex()
                                    .items_center()
                                    .child(
                                        div()
                                             .flex_1()
                                             .min_w_0()
                                             .px(px(14.))
                                            .text_size(px(14.))
                                            .text_color(rgb(palette.text_primary))
                                            .child(SPOTIFY_REDIRECT_URI),
                                    )
                                    .child(
                                        self.icon_button_with(
                                            "copy-spotify-redirect",
                                            "square.on.square",
                                            16.,
                                            SymbolWeight::Regular,
                                        )
                                            .size(px(36.))
                                            .mr(px(6.))
                                            .rounded(px(8.))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    SPOTIFY_REDIRECT_URI.to_owned(),
                                                ));
                                                this.action_notice =
                                                    Some("Redirect URI copied".to_owned());
                                                cx.notify();
                                            })),
                                    ),
                            ),
                    ),
            )
            .child(
                div()
                    .mt(px(28.))
                    .flex()
                    .gap(px(16.))
                    .child(self.onboarding_step_number("2"))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.onboarding_step_title("Add your Client ID"))
                            .child(self.onboarding_detail(
                                "Copy it from Basic Information in your Spotify app.",
                            ))
                            .child(
                                div()
                                    .mt(px(16.))
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(palette.text_primary))
                                    .child("Spotify Client ID"),
                            )
                            .child(
                                div().mt(px(8.)).child(self.spotify_client_id_field()),
                            )
                            .when_some(error, |form, error| {
                                form.child(
                                    div()
                                        .mt(px(8.))
                                        .pl(px(8.))
                                        .border_l_1()
                                        .border_color(rgb(palette.danger))
                                        .text_size(px(12.))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(rgb(palette.text_primary))
                                        .child(error),
                                )
                            })
                            .when(!has_error, |form| {
                                form.child(
                                    div()
                                        .mt(px(8.))
                                        .text_size(px(12.))
                                        .text_color(rgb(palette.text_muted))
                                        .child(
                                            "Client IDs are public. Never enter your client secret.",
                                        ),
                                )
                            })
                            .child(
                                div()
                                    .mt(px(16.))
                                    .flex()
                                    .justify_start()
                                    .child(
                                        self.pill(
                                            "save-spotify-client-id",
                                            "Log in with Spotify",
                                            true,
                                        )
                                        .h(px(48.))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.configure_spotify(window, cx);
                                        })),
                                    ),
                            ),
                    ),
            )
    }

    pub(super) fn spotify_client_id_field(&self) -> Div {
        let palette = self.palette;
        div()
            .h(px(48.))
            .w_full()
            .flex()
            .items_center()
            .px(px(14.))
            .rounded(px(12.))
            .border_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.surface))
            .child(
                Input::new(&self.spotify_client_id_input)
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .px_0()
                    .h_full()
                    .flex_1()
                    .min_w_0(),
            )
    }

    fn spotify_login_form(&mut self, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let connecting = matches!(self.connection_state, ConnectionState::Connecting);
        let configuration_blocked = self.spotify_configuration_blocked;
        div()
            .flex_1()
            .w_full()
            .max_w(px(720.))
            .mx_auto()
            .min_w_0()
            .p(px(if self.compact_layout { 32. } else { 48. }))
            .flex()
            .flex_col()
            .justify_center()
            .child(self.onboarding_task_header(
                "Log in to Spotify",
                "Spotify will ask you to approve Cadence twice: first for your library, then for playback.",
            ))
            .when(!configuration_blocked, |form| {
                form.child(div().mt(px(32.)).flex().justify_start().child(if connecting {
                    self.pill("spotify-login-pending", "Log in with Spotify", true)
                        .h(px(48.))
                        .gap(px(8.))
                        .cursor_default()
                        .child(Spinner::new().small())
                } else {
                    self.pill("spotify-login", "Log in with Spotify", true)
                        .h(px(48.))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.last_error = None;
                            this.connection_state = ConnectionState::Connecting;
                            this.authenticate();
                            cx.notify();
                    }))
                }))
            })
            .when_some(self.last_error.clone(), |form, error| {
                form.child(
                    div()
                        .mt(px(16.))
                        .pl(px(8.))
                        .border_l_1()
                        .border_color(rgb(palette.danger))
                        .text_size(px(13.))
                        .text_color(rgb(palette.text_primary))
                        .child(error),
                )
            })
    }

    fn onboarding_step_number(&self, number: &'static str) -> Div {
        let palette = self.palette;
        div()
            .size(px(32.))
            .flex_none()
            .rounded(px(16.))
            .border_1()
            .border_color(rgb(palette.border))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(13.))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(palette.text_primary))
            .child(number)
    }

    fn onboarding_step_title(&self, title: &'static str) -> Div {
        div()
            .text_size(px(19.))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(self.palette.text_primary))
            .child(title)
    }

    fn onboarding_detail(&self, detail: &'static str) -> Div {
        div()
            .mt(px(8.))
            .text_size(px(14.))
            .text_color(rgb(self.palette.text_muted))
            .child(detail)
    }
}
