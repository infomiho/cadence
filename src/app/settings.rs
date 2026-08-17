use super::onboarding::SPOTIFY_DASHBOARD_URL;
use super::*;

impl CadenceApp {
    pub(super) fn settings_page(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let palette = self.palette;
        let session = self.session.read(cx);
        let saved_configuration = session.client_id_source() == Some(ClientIdSource::Saved);
        let environment_configuration =
            session.client_id_source() == Some(ClientIdSource::Environment);
        let client_id = session.client_id().cloned().unwrap_or_default();
        let dashboard_url = format!("{SPOTIFY_DASHBOARD_URL}/{client_id}");

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
                            .child(self.settings_section_header(
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
                            .child(self.settings_section_header(
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
                                                        components::button(self.palette,
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
                                                                self.settings_button(
                                                                    "settings-change-spotify-app",
                                                                    "Change developer app",
                                                                )
                                                                .on_click(cx.listener(
                                                                    |this, _, _, cx| {
                                                                        this.request_spotify_app_change(cx);
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

    fn settings_section_header(&self, title: &'static str, detail: &'static str) -> Div {
        let palette = self.palette;
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

    fn appearance_option(
        &self,
        id: &'static str,
        icon: &'static str,
        label: &'static str,
        preference: ThemePreference,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let palette = self.palette;
        let selected = appearance::Appearance::preference(cx) == preference;
        components::button(self.palette, id)
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
            .on_click(cx.listener(move |this, _, window, cx| {
                this.set_theme_preference(preference, window, cx);
            }))
    }

    pub(super) fn settings_button(
        &self,
        id: impl Into<ElementId>,
        label: &'static str,
    ) -> Stateful<Div> {
        let palette = self.palette;
        components::button(self.palette, id)
            .h(px(40.))
            .px(px(14.))
            .rounded(px(10.))
            .border_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.surface))
            .text_size(px(13.))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(palette.text_primary))
            .hover(|style| style.bg(rgb(palette.control_hover)))
            .child(label)
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
                                self.settings_button("cancel-spotify-app-change", "Cancel")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.cancel_spotify_app_change(cx);
                                    })),
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
