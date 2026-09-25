use super::*;
use gpui_kit::TestSupportExt as _;

pub(super) const SPOTIFY_DASHBOARD_URL: &str = "https://developer.spotify.com/dashboard";
pub(super) const SPOTIFY_REDIRECT_URI: &str = "http://127.0.0.1:8888/callback";

/// What the setup screens ask the workspace to do.
pub(super) enum OnboardingEvent {
    Authenticate,
    DismissOverlay,
    Notice(SharedString),
    ChangeSpotifyApp,
    RetryBackend,
    ClearError,
}

/// The screens shown before Cadence has a usable Spotify session: setup,
/// sign-in, and the backend failure notice.
pub(super) struct Onboarding {
    session: Entity<session::Session>,
    client_id_input: Entity<InputState>,
    _client_id_subscription: Subscription,
    focus_handle: FocusHandle,
    /// The last error the surrounding window reported, shown alongside the form.
    last_error: Option<SharedString>,
}

impl EventEmitter<OnboardingEvent> for Onboarding {}

impl Onboarding {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let client_id_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("32-character Client ID"));
        let subscription = cx.subscribe_in(
            &client_id_input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    this.session
                        .update(cx, |session, cx| session.clear_setup_error(cx));
                }
                InputEvent::PressEnter { .. } => this.configure(window, cx),
                _ => {}
            },
        );
        Self {
            session: services::AppServices::session(cx),
            client_id_input,
            _client_id_subscription: subscription,
            focus_handle: cx.focus_handle(),
            last_error: None,
        }
    }

    pub(super) fn show_error(&mut self, error: Option<SharedString>, cx: &mut Context<Self>) {
        if self.last_error != error {
            self.last_error = error;
            cx.notify();
        }
    }

    /// Focuses the Client ID field when the session asks for setup.
    pub(super) fn focus_setup_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(
            self.session.read(cx).state(),
            ConnectionState::SetupRequired
        ) && self
            .session
            .update(cx, |session, _| session.take_setup_focus())
        {
            window.focus(&self.client_id_input.read(cx).focus_handle(cx), cx);
        }
    }

    fn configure(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let client_id = self.client_id_input.read(cx).value().trim().to_owned();
        if !valid_client_id(&client_id) {
            self.session.update(cx, |session, cx| {
                session.reject_client_id(
                    "Enter the 32-character Client ID from your Spotify app.",
                    cx,
                )
            });
            window.focus(&self.client_id_input.read(cx).focus_handle(cx), cx);
            cx.notify();
            return;
        }
        self.session
            .update(cx, |session, cx| session.configure(client_id, cx));
        cx.notify();
    }

    fn page(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let palette = appearance::Appearance::palette(cx);
        let context_rail = self.onboarding_context_rail(cx);
        let content = match self.session.read(cx).state() {
            ConnectionState::Failed => self.backend_failure(cx),
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
            .on_action(
                cx.listener(|_, _: &DismissOverlay, _, cx| {
                    cx.emit(OnboardingEvent::DismissOverlay)
                }),
            )
            .size_full()
            .overflow_y_scroll()
            .bg(rgb(palette.surface))
            .font_family("Inter")
            .text_color(rgb(palette.text))
            .child(
                div()
                    .h_full()
                    .min_h(tokens::ONBOARDING_MIN_HEIGHT)
                    .w_full()
                    .flex()
                    .child(context_rail)
                    .child(content),
            )
    }

    fn backend_failure(&self, cx: &mut Context<Self>) -> Div {
        let palette = appearance::Appearance::palette(cx);
        div().flex_1().flex().items_center().justify_center().child(
            div()
                .w(tokens::STATUS_CARD_WIDTH)
                .p_8()
                .rounded_2xl()
                .border_1()
                .border_color(rgb(palette.border))
                .bg(rgb(palette.canvas))
                .child(
                    div()
                        .text_2xl()
                        .font_weight(gpui_kit::FontWeight::BOLD)
                        .text_color(rgb(palette.text_primary))
                        .child("Cadence could not start"),
                )
                .child(
                    div()
                        .mt_3()
                        .text_sm()
                        .line_height(relative(1.5))
                        .text_color(rgb(palette.text_muted))
                        .child(self.last_error.clone().unwrap_or_else(|| {
                            SharedString::new_static("The backend stopped unexpectedly.")
                        })),
                )
                .child(
                    components::settings_button(palette, "retry-backend", "Retry")
                        .mt_6()
                        .on_click(
                            cx.listener(|_, _, _, cx| cx.emit(OnboardingEvent::RetryBackend)),
                        ),
                ),
        )
    }

    fn onboarding_context_rail(&self, cx: &mut Context<Self>) -> Div {
        let palette = appearance::Appearance::palette(cx);
        let show_configuration = matches!(
            self.session.read(cx).state(),
            ConnectionState::AuthorizationRequired | ConnectionState::Connecting
        );
        div()
            .w(tokens::ONBOARDING_RAIL_WIDTH)
            .flex_none()
            .p(tokens::ONBOARDING_RAIL_PADDING)
            .border_r_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.canvas))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(img(services::AppServices::brand_mark(cx)).size_10().flex_none())
                    .child(
                        div()
                            .text_lg()
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .text_color(rgb(palette.text_primary))
                            .child("Cadence"),
                    ),
            )
            .child(
                div()
                    .mt(tokens::ONBOARDING_HEADLINE_OFFSET)
                    .text_3xl()
                    .line_height(relative(1.12))
                    .font_weight(gpui_kit::FontWeight::BOLD)
                    .text_color(rgb(palette.text_primary))
                    .child("Your Spotify,\nin Cadence"),
            )
            .child(
                div()
                    .mt_5()
                    .text_sm()
                    .line_height(relative(1.55))
                    .text_color(rgb(palette.text_muted))
                    .child(
                        "Use a Spotify developer app you control. Cadence stores only its public Client ID.",
                    ),
            )
            .when(show_configuration, |rail| {
                rail.child(self.spotify_login_configuration(cx)).child(
                    div()
                        .mt_3()
                        .text_size(tokens::CAPTION_TEXT)
                        .text_color(rgb(palette.text_muted))
                        .child("No client secret needed. Tokens stay in Keychain."),
                )
            })
            .child(div().flex_1())
    }

    fn spotify_login_configuration(&self, cx: &mut Context<Self>) -> Div {
        let palette = appearance::Appearance::palette(cx);
        let connecting = matches!(self.session.read(cx).state(), ConnectionState::Connecting);
        let client_id = self
            .session
            .read(cx)
            .client_id()
            .cloned()
            .unwrap_or_default();
        let dashboard_url = format!("{SPOTIFY_DASHBOARD_URL}/{client_id}");

        div()
            .mt_8()
            .p_4()
            .rounded_xl()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.surface))
            .child(
                div()
                    .text_size(tokens::CAPTION_TEXT)
                    .text_color(rgb(palette.text_muted))
                    .child("Saved in Cadence"),
            )
            .child(
                div()
                    .mt_1p5()
                    .text_size(tokens::BODY_TEXT)
                    .text_color(rgb(palette.text_primary))
                    .child(client_id),
            )
            .when(
                !connecting
                    && self.session.read(cx).client_id_source() == Some(ClientIdSource::Saved),
                |card| {
                    card.child(
                        div().mt_3p5().flex().justify_start().child(
                            components::settings_button(
                                palette,
                                "login-change-spotify-app",
                                "Change developer app",
                            )
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(OnboardingEvent::ChangeSpotifyApp);
                            })),
                        ),
                    )
                },
            )
            .when(
                self.session.read(cx).client_id_source() == Some(ClientIdSource::Environment),
                |card| {
                    card.child(
                        components::settings_button(
                            palette,
                            "login-open-spotify-dashboard",
                            "Open dashboard",
                        )
                        .mt_3p5()
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.open_url(&dashboard_url);
                        })),
                    )
                    .child(
                        div()
                            .mt_2p5()
                            .text_size(tokens::CAPTION_TEXT)
                            .line_height(relative(1.4))
                            .text_color(rgb(palette.text_muted))
                            .child("Change SPOTIFY_CLIENT_ID outside Cadence."),
                    )
                },
            )
    }

    fn onboarding_task_header(
        palette: CadencePalette,
        title: &'static str,
        detail: &'static str,
    ) -> Div {
        div()
            .child(
                div()
                    .text_size(tokens::TASK_TITLE_TEXT)
                    .font_weight(gpui_kit::FontWeight::BOLD)
                    .text_color(rgb(palette.text_primary))
                    .child(title),
            )
            .child(
                div()
                    .mt_2()
                    .text_sm()
                    .text_color(rgb(palette.text_muted))
                    .child(detail),
            )
    }

    fn spotify_setup_form(&mut self, cx: &mut Context<Self>) -> Div {
        let palette = appearance::Appearance::palette(cx);
        let error = self.session.read(cx).setup_error().cloned();
        let has_error = error.is_some();
        div()
            .flex_1()
            .w_full()
            .max_w(tokens::ONBOARDING_FORM_MAX_WIDTH)
            .mx_auto()
            .min_w_0()
            .p_12()
            .flex()
            .flex_col()
            .child(Self::onboarding_task_header(palette, "Set up Spotify", "Two steps, about two minutes."))
            .child(
                div()
                    .mt_7()
                    .flex()
                    .gap_4()
                    .child(Self::onboarding_step_number(palette, "1"))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Self::onboarding_step_title(palette, "Create a Spotify app"))
                            .child(Self::onboarding_detail(palette,
                                "Select Web API in the Spotify developer dashboard.",
                            ))
                            .child(
                                components::settings_button(palette,
                                    "open-spotify-dashboard",
                                    "Open Spotify dashboard",
                                )
                                    .mt_4()
                                    .w(tokens::ONBOARDING_ACTION_WIDTH)
                                    .on_click(cx.listener(|_, _, _, cx| {
                                        cx.open_url(SPOTIFY_DASHBOARD_URL);
                                    })),
                            )
                            .child(
                                div()
                                    .mt_6()
                                    .text_sm()
                                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                    .text_color(rgb(palette.text_primary))
                                    .child("Add this redirect URI"),
                            )
                            .child(
                                div()
                                    .mt_2()
                                    .h_12()
                                    .rounded_xl()
                                    .border_1()
                                    .border_color(rgb(palette.border))
                                    .flex()
                                    .items_center()
                                    .child(
                                        div()
                                             .flex_1()
                                             .min_w_0()
                                             .px_3p5()
                                            .text_sm()
                                            .text_color(rgb(palette.text_primary))
                                            .child(SPOTIFY_REDIRECT_URI),
                                    )
                                    .child(
                                        components::icon_button_sized(appearance::Appearance::palette(cx),
                                            "copy-spotify-redirect",
                                            CadenceIcon::Copy,
                                            tokens::FIELD_ICON,
                                        )
                                            .size_9()
                                            .mr_1p5()
                                            .rounded_lg()
                                            .on_click(cx.listener(|_, _, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    SPOTIFY_REDIRECT_URI.to_owned(),
                                                ));
                                                cx.emit(OnboardingEvent::Notice(
                                                    SharedString::new_static("Redirect URI copied"),
                                                ));
                                            })),
                                    ),
                            ),
                    ),
            )
            .child(
                div()
                    .mt_7()
                    .flex()
                    .gap_4()
                    .child(Self::onboarding_step_number(palette, "2"))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Self::onboarding_step_title(palette, "Add your Client ID"))
                            .child(Self::onboarding_detail(palette,
                                "Copy it from Basic Information in your Spotify app.",
                            ))
                            .child(
                                div()
                                    .mt_4()
                                    .text_sm()
                                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                    .text_color(rgb(palette.text_primary))
                                    .child("Spotify Client ID"),
                            )
                            .child(
                                div().mt_2().child(self.spotify_client_id_field(palette)),
                            )
                            .when_some(error, |form, error| {
                                form.child(
                                    div()
                                        .mt_2()
                                        .pl_2()
                                        .border_l_1()
                                        .border_color(rgb(palette.danger))
                                        .text_xs()
                                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                        .text_color(rgb(palette.text_primary))
                                        .child(error),
                                )
                            })
                            .when(!has_error, |form| {
                                form.child(
                                    div()
                                        .mt_2()
                                        .text_xs()
                                        .text_color(rgb(palette.text_muted))
                                        .child(
                                            "Client IDs are public. Never enter your client secret.",
                                        ),
                                )
                            })
                            .child(
                                div()
                                    .mt_4()
                                    .flex()
                                    .justify_start()
                                    .child(
                                        components::pill(appearance::Appearance::palette(cx),
                                            "save-spotify-client-id",
                                            "Log in with Spotify",
                                            true,
                                        )
                                        .test_support()
                                        .h_12()
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.configure(window, cx);
                                        })),
                                    ),
                            ),
                    ),
            )
    }

    fn spotify_client_id_field(&self, palette: CadencePalette) -> Div {
        div()
            .h_12()
            .w_full()
            .flex()
            .items_center()
            .px_3p5()
            .rounded_xl()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.surface))
            .child(
                Input::new(&self.client_id_input)
                    .id("client-id-input")
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
        let palette = appearance::Appearance::palette(cx);
        let connecting = matches!(self.session.read(cx).state(), ConnectionState::Connecting);
        let configuration_blocked = self.session.read(cx).configuration_blocked();
        div()
            .flex_1()
            .w_full()
            .max_w(tokens::ONBOARDING_FORM_MAX_WIDTH)
            .mx_auto()
            .min_w_0()
            .p_12()
            .flex()
            .flex_col()
            .justify_center()
            .child(Self::onboarding_task_header(palette,
                "Log in to Spotify",
                "Spotify will ask you to approve Cadence twice: first for your library, then for playback.",
            ))
            .when(!configuration_blocked, |form| {
                form.child(div().mt_8().flex().justify_start().child(if connecting {
                    components::pill(appearance::Appearance::palette(cx), "spotify-login-pending", "Log in with Spotify", true)
                        .h_12()
                        .gap_2()
                        .cursor_default()
                        .child(Spinner::new().small())
                } else {
                    components::pill(appearance::Appearance::palette(cx), "spotify-login", "Log in with Spotify", true)
                        .h_12()
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.emit(OnboardingEvent::ClearError);
                            this.session
                                .update(cx, |session, cx| session.set_connecting(cx));
                            cx.emit(OnboardingEvent::Authenticate);
                            cx.notify();
                        }))
                }))
            })
            .when_some(self.last_error.clone(), |form, error| {
                form.child(
                    div()
                        .mt_4()
                        .pl_2()
                        .border_l_1()
                        .border_color(rgb(palette.danger))
                        .text_size(tokens::BODY_TEXT)
                        .text_color(rgb(palette.text_primary))
                        .child(error),
                )
            })
    }

    fn onboarding_step_number(palette: CadencePalette, number: &'static str) -> Div {
        div()
            .size_8()
            .flex_none()
            .rounded_full()
            .border_1()
            .border_color(rgb(palette.border))
            .flex()
            .items_center()
            .justify_center()
            .text_size(tokens::BODY_TEXT)
            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
            .text_color(rgb(palette.text_primary))
            .child(number)
    }

    fn onboarding_step_title(palette: CadencePalette, title: &'static str) -> Div {
        div()
            .text_size(tokens::STEP_TITLE_TEXT)
            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
            .text_color(rgb(palette.text_primary))
            .child(title)
    }

    fn onboarding_detail(palette: CadencePalette, detail: &'static str) -> Div {
        div()
            .mt_2()
            .text_sm()
            .text_color(rgb(palette.text_muted))
            .child(detail)
    }
}

impl Render for Onboarding {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.page(cx)
    }
}
