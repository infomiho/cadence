use super::*;
use gpui_kit::TestSupportExt as _;

/// What the toolbar asks the workspace to do.
pub(super) enum ToolbarEvent {
    QueryChanged(String),
    SubmitSearch,
    Navigate(Route),
    OpenSettings,
    Connect,
    Logout,
    /// The account menu opened, so anything else overlaying the page should go.
    MenuOpened,
}

/// The bar above the page: search on the left, account on the right.
pub(super) struct Toolbar {
    search_input: Entity<InputState>,
    menu_open: bool,
    session: Entity<session::Session>,
    player: Entity<player::Player>,
    route: Route,
    /// Where the back button goes, when the current route has one.
    back_target: Option<Route>,
    /// The workspace's standing failure, shown under the account name.
    error: Option<SharedString>,
    _search_subscription: Subscription,
}

impl EventEmitter<ToolbarEvent> for Toolbar {}

impl Toolbar {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search Spotify"));
        let search_subscription = cx.subscribe_in(
            &search_input,
            window,
            |_, input, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    let query = input.read(cx).value().to_string();
                    cx.emit(ToolbarEvent::QueryChanged(query));
                }
                InputEvent::PressEnter { .. } => cx.emit(ToolbarEvent::SubmitSearch),
                _ => {}
            },
        );
        Self {
            search_input,
            menu_open: false,
            session: services::AppServices::session(cx),
            player: services::AppServices::player(cx),
            route: Route::LikedSongs,
            back_target: None,
            error: None,
            _search_subscription: search_subscription,
        }
    }

    /// Pushes the workspace state the toolbar reflects but does not own.
    pub(super) fn show(
        &mut self,
        route: Route,
        back_target: Option<Route>,
        error: Option<SharedString>,
        cx: &mut Context<Self>,
    ) {
        if self.route != route || self.back_target != back_target || self.error != error {
            self.route = route;
            self.back_target = back_target;
            self.error = error;
            cx.notify();
        }
    }

    pub(super) fn focus_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.search_input.read(cx).focus_handle(cx), cx);
    }

    pub(super) fn clear_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    pub(super) fn close_menu(&mut self, cx: &mut Context<Self>) {
        if self.menu_open {
            self.menu_open = false;
            cx.notify();
        }
    }

    fn search_field(
        &self,
        palette: CadencePalette,
        compact: bool,
        window: &Window,
        cx: &App,
    ) -> impl IntoElement {
        let frame = div()
            .w(if compact {
                tokens::COMPACT_SEARCH_FIELD_WIDTH
            } else {
                tokens::SEARCH_FIELD_WIDTH
            })
            .h_10()
            .flex()
            .items_center()
            .justify_start()
            .gap_2p5()
            .px_3p5()
            .rounded_xl()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.surface))
            .text_sm()
            .text_color(rgb(palette.text_muted));
        let focused = self
            .search_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        components::text_field_frame(palette, frame, focused)
            .id("search-field")
            .child(components::icon(
                CadenceIcon::Search,
                tokens::FIELD_ICON,
                palette.text_muted,
            ))
            .child(
                Input::new(&self.search_input)
                    .id("search-input")
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .flex_1()
                    .min_w_0()
                    .h_full(),
            )
            .child(
                div()
                    .px_1p5()
                    .py_0p5()
                    .rounded_md()
                    .bg(rgb(palette.surface_raised))
                    .text_color(rgb(palette.text))
                    .text_size(tokens::CAPTION_TEXT)
                    .child("⌘ K"),
            )
    }

    /// The line under the account name: the newest failure if there is one,
    /// otherwise how far the Spotify connection has got.
    fn account_detail(&self, cx: &App) -> SharedString {
        if let Some(error) = self.player.read(cx).error() {
            return error.clone();
        }
        if let Some(error) = &self.error {
            return error.clone();
        }
        match self.session.read(cx).state() {
            ConnectionState::Starting => "Starting Spotify…".into(),
            ConnectionState::Failed => "Backend unavailable".into(),
            ConnectionState::SetupRequired => "Developer app required".into(),
            ConnectionState::AuthorizationRequired => "Not connected".into(),
            ConnectionState::Connecting => "Connecting…".into(),
            ConnectionState::Ready => "Spotify connected".into(),
        }
    }

    fn profile_name(&self, cx: &App) -> SharedString {
        self.session.read(cx).profile().map_or_else(
            || SharedString::new_static("Spotify account"),
            |profile| profile.display_name.clone(),
        )
    }

    fn account_menu(&self, palette: CadencePalette, cx: &mut Context<Self>) -> impl IntoElement {
        let profile_name = self.profile_name(cx);
        let can_connect = matches!(
            self.session.read(cx).state(),
            ConnectionState::AuthorizationRequired
        );

        components::menu_surface(palette)
            .on_mouse_up_out(
                gpui_kit::MouseButton::Left,
                cx.listener(|this, _, _, cx| this.close_menu(cx)),
            )
            .absolute()
            .top_12()
            .right_0()
            .child(
                div()
                    .px_2p5()
                    .py_2()
                    .child(
                        div()
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .text_color(rgb(palette.text_primary))
                            .child(profile_name),
                    )
                    .child(
                        div()
                            .mt_0p5()
                            .text_xs()
                            .text_color(rgb(palette.text_muted))
                            .child(self.account_detail(cx)),
                    ),
            )
            .when(can_connect, |menu| {
                menu.child(
                    components::menu_item(
                        palette,
                        "account-connect",
                        CadenceIcon::Key,
                        "Log in with Spotify",
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.menu_open = false;
                        cx.emit(ToolbarEvent::Connect);
                        cx.notify();
                    })),
                )
            })
            .child(
                div()
                    .mt_1()
                    .pt_1p5()
                    .border_t_1()
                    .border_color(rgb(palette.border))
                    .child(
                        components::menu_item(
                            palette,
                            "account-settings",
                            CadenceIcon::Settings,
                            "Settings",
                            false,
                        )
                        .test_support()
                        .on_click(cx.listener(|_, _, _, cx| {
                            cx.emit(ToolbarEvent::OpenSettings);
                        })),
                    ),
            )
            .when(self.session.read(cx).is_ready(), |menu| {
                menu.child(
                    components::menu_item(
                        palette,
                        "account-logout",
                        CadenceIcon::SignOut,
                        "Logout",
                        true,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.menu_open = false;
                        cx.emit(ToolbarEvent::Logout);
                        cx.notify();
                    })),
                )
            })
    }
}

impl Render for Toolbar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::Appearance::palette(cx);
        let compact = uses_compact_content_layout(window.viewport_size().width, window.rem_size());
        let profile_name = self.profile_name(cx);
        let profile = self.session.read(cx).profile().cloned();
        let profile_artwork = profile
            .as_ref()
            .and_then(|profile| profile.artwork_url.as_deref());
        let showing_settings = self.route == Route::Settings;

        div()
            .h(tokens::TOOLBAR_HEIGHT)
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .px_7()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2p5()
                    .when_some(self.back_target, |group, origin| {
                        group.child(
                            components::icon_button(
                                palette,
                                "detail-back",
                                CadenceIcon::ChevronLeft,
                            )
                            .on_click(cx.listener(
                                move |_, _, _, cx| {
                                    cx.emit(ToolbarEvent::Navigate(origin));
                                },
                            )),
                        )
                    })
                    .when(showing_settings, |group| {
                        group.child(
                            div()
                                .h_10()
                                .flex()
                                .items_center()
                                .text_lg()
                                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                .text_color(rgb(palette.text_primary))
                                .child("Settings"),
                        )
                    })
                    .when(!showing_settings, |group| {
                        group.child(self.search_field(palette, compact, window, cx))
                    }),
            )
            .child(
                div()
                    .relative()
                    .child(
                        components::filled_button(palette, "account")
                            .test_support()
                            .size_10()
                            .rounded_full()
                            .text_color(rgb(palette.on_accent))
                            .text_xs()
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .child(components::profile_avatar(
                                profile_artwork,
                                components::initials(&profile_name),
                                window,
                            ))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.menu_open = !this.menu_open;
                                if this.menu_open {
                                    cx.emit(ToolbarEvent::MenuOpened);
                                }
                                cx.notify();
                            })),
                    )
                    .when(self.menu_open, |anchor| {
                        anchor.child(deferred(self.account_menu(palette, cx)))
                    }),
            )
    }
}

/// Asks before throwing away the saved Spotify developer app, which cannot be
/// undone from inside Cadence.
pub(super) fn spotify_app_change_confirmation(
    palette: CadencePalette,
    signed_in: bool,
    cancel: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static,
    confirm: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let consequence = if signed_in {
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
                .w(tokens::DIALOG_WIDTH)
                .p_6()
                .rounded_2xl()
                .border_1()
                .border_color(rgb(palette.border))
                .bg(rgb(palette.surface))
                .shadow_lg()
                .child(
                    div()
                        .text_xl()
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .text_color(rgb(palette.text_primary))
                        .child("Change Spotify developer app?"),
                )
                .child(
                    div()
                        .mt_2p5()
                        .text_sm()
                        .line_height(relative(1.5))
                        .text_color(rgb(palette.text))
                        .child(consequence),
                )
                .child(
                    div()
                        .mt_6()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            components::settings_button(
                                palette,
                                "cancel-spotify-app-change",
                                "Cancel",
                            )
                            .test_support()
                            .on_click(cancel),
                        )
                        .child(
                            components::filled_button(palette, "confirm-spotify-app-change")
                                .h_10()
                                .px_3p5()
                                .rounded(tokens::CONTROL_RADIUS)
                                .bg(rgb(palette.destructive))
                                .text_size(tokens::BODY_TEXT)
                                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                .text_color(rgb(palette.on_destructive))
                                .hover(|style| style.opacity(0.88))
                                .child("Change developer app")
                                .on_click(confirm),
                        ),
                ),
        )
}
