use super::*;

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
    error: Option<String>,
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
        error: Option<String>,
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
        window.focus(&self.search_input.read(cx).focus_handle(cx));
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

    fn search_field(&self, palette: CadencePalette, compact: bool) -> impl IntoElement {
        div()
            .id("search-field")
            .w(px(if compact { 340. } else { 520. }))
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
            )
    }

    /// The line under the account name: the newest failure if there is one,
    /// otherwise how far the Spotify connection has got.
    fn account_detail(&self, cx: &App) -> SharedString {
        if let Some(error) = self.player.read(cx).error() {
            return error.clone().into();
        }
        if let Some(error) = &self.error {
            return error.clone().into();
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

    fn profile_name(&self, cx: &App) -> String {
        self.session.read(cx).profile().map_or_else(
            || "Spotify account".to_owned(),
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
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| this.close_menu(cx)),
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
                            .child(self.account_detail(cx)),
                    ),
            )
            .when(can_connect, |menu| {
                menu.child(
                    components::menu_item(
                        palette,
                        "account-connect",
                        "key",
                        "Log in with Spotify",
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.menu_open = false;
                        cx.emit(ToolbarEvent::Connect);
                        cx.notify();
                    })),
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
                            palette,
                            "account-settings",
                            "gearshape",
                            "Settings",
                            false,
                        )
                        .on_click(cx.listener(|_, _, _, cx| {
                            cx.stop_propagation();
                            cx.emit(ToolbarEvent::OpenSettings);
                        })),
                    ),
            )
            .when(self.session.read(cx).is_ready(), |menu| {
                menu.child(
                    components::menu_item(
                        palette,
                        "account-logout",
                        "rectangle.portrait.and.arrow.right",
                        "Logout",
                        true,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
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
        let compact = uses_compact_content_layout(f32::from(window.viewport_size().width));
        let profile_name = self.profile_name(cx);
        let profile = self.session.read(cx).profile().cloned();
        let profile_artwork = profile
            .as_ref()
            .and_then(|profile| profile.artwork_url.as_deref());
        let showing_settings = self.route == Route::Settings;

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
                    .when_some(self.back_target, |group, origin| {
                        group.child(
                            components::icon_button(palette, "detail-back", "chevron.left")
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.emit(ToolbarEvent::Navigate(origin));
                                })),
                        )
                    })
                    .when(showing_settings, |group| {
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
                    .when(!showing_settings, |group| {
                        group.child(self.search_field(palette, compact))
                    }),
            )
            .child(
                div()
                    .relative()
                    .child(
                        components::button(palette, "account")
                            .size(px(40.))
                            .rounded(px(40.))
                            .overflow_hidden()
                            .text_color(rgb(palette.on_accent))
                            .text_size(px(12.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(components::profile_avatar(
                                profile_artwork,
                                components::initials(&profile_name),
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
    cancel: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    confirm: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
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
                                palette,
                                "cancel-spotify-app-change",
                                "Cancel",
                            )
                            .on_click(cancel),
                        )
                        .child(
                            components::button(palette, "confirm-spotify-app-change")
                                .h(px(40.))
                                .px(px(14.))
                                .rounded(px(10.))
                                .bg(rgb(palette.destructive))
                                .text_size(px(13.))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(palette.on_destructive))
                                .hover(|style| style.opacity(0.88))
                                .child("Change developer app")
                                .on_click(confirm),
                        ),
                ),
        )
}
