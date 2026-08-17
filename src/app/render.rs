use super::*;

// Render and row-building code derives UI from memory; all I/O stays behind the backend or image cache.
impl Render for CadenceApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        self.compact_layout = uses_compact_content_layout(f32::from(window.viewport_size().width));
        let action_notice = self.action_notice.clone().map(|message| {
            deferred(
                div()
                    .occlude()
                    .absolute()
                    .top(px(76.))
                    .right(px(24.))
                    .w(px(360.))
                    .min_h(px(48.))
                    .px(px(14.))
                    .py(px(8.))
                    .rounded(px(14.))
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(rgb(palette.surface_raised))
                    .shadow_lg()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .text_size(px(13.))
                    .text_color(rgb(palette.text_primary))
                    .child(div().flex_1().child(message))
                    .child(
                        self.icon_button("dismiss-action-notice", "xmark")
                            .size(px(32.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.action_notice = None;
                                cx.notify();
                            })),
                    ),
            )
        });
        if !matches!(self.connection_state, ConnectionState::Ready) {
            if matches!(self.connection_state, ConnectionState::SetupRequired)
                && self.spotify_setup_needs_focus
            {
                window.focus(&self.spotify_client_id_input.read(cx).focus_handle(cx));
                self.spotify_setup_needs_focus = false;
            }
            return self
                .onboarding_page(cx)
                .relative()
                .when_some(action_notice, |root, notice| root.child(notice))
                .when(self.spotify_app_change_confirmation_open, |root| {
                    root.child(deferred(self.spotify_app_change_confirmation(cx)))
                })
                .into_any_element();
        }
        let page = match self.route {
            Route::LikedSongs => self.liked_songs_page(cx).into_any_element(),
            Route::Favorites => self.favorites_page(cx).into_any_element(),
            Route::Recent => self.recent_page(cx).into_any_element(),
            Route::Search => self.search_page(cx).into_any_element(),
            Route::Playlists => self.playlists_page(cx).into_any_element(),
            Route::Playlist => self.playlist_page(cx).into_any_element(),
            Route::Artist => self.artist_page(cx).into_any_element(),
            Route::Album => self.album_page(cx).into_any_element(),
            Route::Settings => self.settings_page(cx).into_any_element(),
        };

        div()
            .id("cadence-root")
            .key_context("Cadence")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_tab))
            .on_action(cx.listener(Self::on_tab_prev))
            .on_action(cx.listener(Self::open_search))
            .on_action(cx.listener(Self::close_window))
            .on_action(cx.listener(Self::dismiss_overlay))
            .on_action(cx.listener(Self::toggle_playback))
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.player.update(cx, |player, cx| {
                        if player.volume_dragging() {
                            player.drag_volume(event.position.x, window, cx);
                        }
                    });
                }),
            )
            .on_mouse_up(gpui::MouseButton::Left, cx.listener(Self::end_volume_drag))
            .on_mouse_up_out(gpui::MouseButton::Left, cx.listener(Self::end_volume_drag))
            .on_scroll_wheel(cx.listener(|this, _, _, cx| {
                if this.track_menu_open.take().is_some() {
                    cx.notify();
                }
            }))
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(palette.canvas))
            .font_family("Inter")
            .text_color(rgb(palette.text))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.sidebar(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .bg(rgb(palette.surface))
                            .child(self.toolbar(cx))
                            .child(div().flex_1().min_h_0().overflow_hidden().child(page)),
                    )
                    .when(self.queue_open, |root| root.child(self.queue_drawer(cx))),
            )
            .child(self.player_bar(window, cx))
            .when_some(action_notice, |root, notice| root.child(notice))
            .when(self.spotify_app_change_confirmation_open, |root| {
                root.child(deferred(self.spotify_app_change_confirmation(cx)))
            })
            .into_any_element()
    }
}
