use super::*;

#[derive(Clone)]
struct MascotOption {
    label: &'static str,
    preference: MascotPreference,
}

impl SelectItem for MascotOption {
    type Value = MascotPreference;

    fn title(&self) -> SharedString {
        self.label.into()
    }

    fn value(&self) -> &Self::Value {
        &self.preference
    }
}

fn mascot_options() -> Vec<MascotOption> {
    vec![
        MascotOption {
            label: "None",
            preference: MascotPreference::None,
        },
        MascotOption {
            label: "Vespa Romeo",
            preference: MascotPreference::RomeoVespa,
        },
        MascotOption {
            label: "Vespa Duo",
            preference: MascotPreference::VespaDuo,
        },
        MascotOption {
            label: "Tarantella Dancer",
            preference: MascotPreference::TarantellaDancer,
        },
    ]
}

/// What the settings page asks the workspace to do.
pub(super) enum SettingsEvent {
    RequestAppChange,
    SetTheme(ThemePreference),
    SetAutoplay(bool),
    SetMascot(MascotPreference),
}

/// The settings page: appearance, playback, and the Spotify developer app.
pub(super) struct Settings {
    session: Entity<session::Session>,
    mascot_select: Entity<SelectState<Vec<MascotOption>>>,
}

impl EventEmitter<SettingsEvent> for Settings {}

impl Settings {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mascot = services::AppServices::preferences(cx).mascot;
        let mascot_select = cx.new(|cx| {
            SelectState::new(
                mascot_options(),
                Some(IndexPath::default().row(mascot_index(mascot))),
                window,
                cx,
            )
        });
        cx.subscribe_in(
            &mascot_select,
            window,
            |_, _, event: &SelectEvent<Vec<MascotOption>>, _, cx| {
                let SelectEvent::Confirm(Some(mascot)) = event else {
                    return;
                };
                cx.emit(SettingsEvent::SetMascot(*mascot));
            },
        )
        .detach();
        Self {
            session: services::AppServices::session(cx),
            mascot_select,
        }
    }

    pub(super) fn sync_mascot(
        &mut self,
        mascot: MascotPreference,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mascot_select.update(cx, |select, cx| {
            select.set_selected_index(
                Some(IndexPath::default().row(mascot_index(mascot))),
                window,
                cx,
            );
        });
    }

    fn page(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let palette = appearance::Appearance::palette(cx);
        let session = self.session.read(cx);
        let saved_configuration = session.client_id_source() == Some(ClientIdSource::Saved);
        let environment_configuration =
            session.client_id_source() == Some(ClientIdSource::Environment);
        let client_id = session.client_id().cloned().unwrap_or_default();
        let dashboard_url = format!("{}/{client_id}", onboarding::SPOTIFY_DASHBOARD_URL);
        let preferences = services::AppServices::preferences(cx);
        let autoplay = preferences.autoplay;

        div()
            .id("settings-page")
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .max_w(px(760.))
                    .mx_auto()
                    .px(px(40.))
                    .pt(px(28.))
                    .pb(px(64.))
                    .child(
                        div()
                            .child(Self::settings_section_header(
                                palette,
                                "Appearance",
                                "Choose how Cadence looks on this Mac.",
                            ))
                            .child(
                                div()
                                    .mt(px(16.))
                                    .p(px(4.))
                                    .rounded(px(14.))
                                    .bg(rgb(palette.control))
                                    .flex()
                                    .gap(px(4.))
                                    .child(self.appearance_option(
                                        "settings-appearance-system",
                                        "circle.lefthalf.filled",
                                        "System",
                                        ThemePreference::System,
                                        cx,
                                    ))
                                    .child(self.appearance_option(
                                        "settings-appearance-light",
                                        "sun.max",
                                        "Light",
                                        ThemePreference::Light,
                                        cx,
                                    ))
                                    .child(self.appearance_option(
                                        "settings-appearance-dark",
                                        "moon",
                                        "Dark",
                                        ThemePreference::Dark,
                                        cx,
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(48.))
                            .child(Self::settings_section_header(
                                palette,
                                "Playback",
                                "Control playback behavior and player-bar extras.",
                            ))
                            .child(
                                div()
                                    .mt(px(16.))
                                    .rounded(px(16.))
                                    .border_1()
                                    .border_color(rgb(palette.border))
                                    .bg(rgb(palette.surface_raised))
                                    .p(px(20.))
                                    .child(Self::playback_setting_row(
                                        palette,
                                        "Autoplay",
                                        "Keep the music going with similar songs when the queue ends.",
                                        Switch::new("settings-autoplay").checked(autoplay).on_click(
                                            cx.listener(|_, checked: &bool, _, cx| {
                                                cx.emit(SettingsEvent::SetAutoplay(*checked));
                                            }),
                                        ),
                                    ))
                                    .child(
                                        div()
                                            .mt(px(20.))
                                            .pt(px(20.))
                                            .border_t_1()
                                            .border_color(rgb(palette.border))
                                            .child(Self::playback_setting_row(
                                                palette,
                                                "Mascot",
                                                "Choose which mascot appears above the player bar.",
                                                Select::new(&self.mascot_select)
                                                    .w(px(220.))
                                                    .menu_width(px(220.)),
                                            )),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(48.))
                            .child(Self::settings_section_header(
                                palette,
                                "Spotify",
                                "Manage the developer app Cadence uses to connect to Spotify.",
                            ))
                            .child(
                                div()
                                    .mt(px(16.))
                                    .rounded(px(16.))
                                    .border_1()
                                    .border_color(rgb(palette.border))
                                    .bg(rgb(palette.surface_raised))
                                    .p(px(20.))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .gap(px(20.))
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .child(
                                                        div()
                                                            .text_size(px(12.))
                                                            .font_weight(
                                                                gpui::FontWeight::MEDIUM,
                                                            )
                                                            .text_color(rgb(palette.text))
                                                            .child(if environment_configuration {
                                                                "Configured by SPOTIFY_CLIENT_ID"
                                                            } else {
                                                                "Spotify developer app"
                                                            }),
                                                    )
                                                    .child(
                                                        components::button(palette,
                                                            "settings-open-spotify-dashboard",
                                                        )
                                                            .mt(px(6.))
                                                            .text_size(px(14.))
                                                            .text_color(rgb(palette.link))
                                                            .underline()
                                                            .gap(px(5.))
                                                            .justify_start()
                                                            .child(client_id)
                                                            .child(components::icon(
                                                                "arrow.up.right",
                                                                11.,
                                                                palette.link,
                                                            ))
                                                            .on_click(cx.listener(
                                                                move |_, _, _, cx| {
                                                                    cx.open_url(&dashboard_url);
                                                                },
                                                            )),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_none()
                                                    .gap(px(8.))
                                                    .when(
                                                        saved_configuration,
                                                        |actions| {
                                                            actions.child(
                                                                components::settings_button(palette,
                                                                    "settings-change-spotify-app",
                                                                    "Change developer app",
                                                                )
                                                                .on_click(cx.listener(
                                                                    |_, _, _, cx| {
                                                                        cx.emit(
                                                                            SettingsEvent::RequestAppChange,
                                                                        );
                                                                    },
                                                                )),
                                                            )
                                                        },
                                                    ),
                                            ),
                                    )
                                    .when(environment_configuration, |card| {
                                        card.child(
                                            div()
                                                .mt(px(12.))
                                                .text_size(px(13.))
                                                .line_height(relative(1.45))
                                                .text_color(rgb(palette.text))
                                                .child(
                                                    "Quit Cadence and update the environment variable to change the developer app.",
                                                ),
                                        )
                                    }),
                            ),
                    ),
            )
    }

    fn settings_section_header(
        palette: CadencePalette,
        title: &'static str,
        detail: &'static str,
    ) -> Div {
        div()
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(palette.text_primary))
                    .child(title),
            )
            .child(
                div()
                    .mt(px(4.))
                    .text_size(px(13.))
                    .text_color(rgb(palette.text_muted))
                    .child(detail),
            )
    }

    fn playback_setting_row(
        palette: CadencePalette,
        title: &'static str,
        description: &'static str,
        field: impl IntoElement,
    ) -> Div {
        h_flex()
            .w_full()
            .justify_between()
            .items_start()
            .gap(px(12.))
            .child(
                v_flex()
                    .flex_1()
                    .max_w(relative(0.6))
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(palette.text))
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .line_height(relative(1.45))
                            .text_color(rgb(palette.text_muted))
                            .child(description),
                    ),
            )
            .child(div().child(field))
    }

    fn appearance_option(
        &self,
        id: &'static str,
        icon: &'static str,
        label: &'static str,
        preference: ThemePreference,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let palette = appearance::Appearance::palette(cx);
        let selected = appearance::Appearance::preference(cx) == preference;
        components::button(palette, id)
            .h(px(44.))
            .flex_1()
            .gap(px(8.))
            .rounded(px(10.))
            .bg(if selected {
                rgb(palette.selection)
            } else {
                rgb(palette.control)
            })
            .text_size(px(14.))
            .font_weight(if selected {
                gpui::FontWeight::SEMIBOLD
            } else {
                gpui::FontWeight::MEDIUM
            })
            .text_color(rgb(palette.text_primary))
            .hover(|style| style.bg(rgb(palette.control_hover)))
            .child(components::icon(icon, 15., palette.text_primary))
            .child(label)
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.emit(SettingsEvent::SetTheme(preference));
            }))
    }
}

fn mascot_index(mascot: MascotPreference) -> usize {
    match mascot {
        MascotPreference::None => 0,
        MascotPreference::RomeoVespa => 1,
        MascotPreference::VespaDuo => 2,
        MascotPreference::TarantellaDancer => 3,
    }
}

impl Render for Settings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.page(cx)
    }
}
