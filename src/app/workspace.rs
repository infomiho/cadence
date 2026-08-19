use super::*;

use chrome::ToolbarEvent;
use router::Router;

/// The window's root: it owns where the listener is, composes the views around
/// the current page, and carries the failures and notices that outlive any one
/// of them. Everything it shows is a view entity of its own.
pub(super) struct Workspace {
    pub(super) router: Router,
    pub(super) last_error: Option<String>,
    pub(super) action_notice: Option<String>,
    pub(super) radio_request_id: u64,
    pub(super) pending_radio_request: Option<u64>,
    pub(super) player: Entity<player::Player>,
    pub(super) session: Entity<session::Session>,
    pub(super) library: Entity<library::Library>,
    pub(super) liked_songs: Entity<library_pages::LibraryTracksPage>,
    pub(super) favorites: Entity<library_pages::LibraryTracksPage>,
    pub(super) recent: Entity<library_pages::LibraryTracksPage>,
    pub(super) playlists: Entity<library_pages::PlaylistsPage>,
    pub(super) search: Entity<catalog::SearchPage>,
    pub(super) playlist: Entity<catalog::PlaylistPage>,
    pub(super) artist: Entity<catalog::ArtistPage>,
    pub(super) album: Entity<catalog::AlbumPage>,
    pub(super) onboarding: Entity<onboarding::Onboarding>,
    pub(super) settings: Entity<settings::Settings>,
    pub(super) sidebar: Entity<sidebar::Sidebar>,
    pub(super) toolbar: Entity<chrome::Toolbar>,
    pub(super) player_bar: Entity<player_bar::PlayerBar>,
    pub(super) queue_drawer: Entity<player_bar::QueueDrawer>,
    pub(super) focus_handle: FocusHandle,
    pub(super) _appearance_subscription: Subscription,
}

impl Workspace {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let preferences = services::AppServices::preferences(cx);
        let focus_handle = cx.focus_handle();
        appearance::Appearance::attach(window, cx);
        let appearance_subscription = cx.observe_window_appearance(window, |this, window, cx| {
            this.update_system_appearance(window, cx);
        });
        window.focus(&focus_handle);
        let player = services::AppServices::player(cx);
        cx.subscribe(&player, |this, _, _: &player::PlaybackUnavailable, cx| {
            this.last_error = Some("Cadence backend is busy or not running".to_owned());
            cx.notify();
        })
        .detach();
        let session = services::AppServices::session(cx);
        cx.subscribe_in(
            &session,
            window,
            |this, _, event: &session::SessionEvent, window, cx| {
                this.handle_session_event(event, window, cx)
            },
        )
        .detach();
        let library = services::AppServices::library(cx);
        cx.subscribe(&library, |this, _, _: &library::LibraryLoaded, cx| {
            this.last_error = None;
            cx.notify();
        })
        .detach();
        let backend = services::AppServices::backend(cx);
        let library_page = |section, cx: &mut Context<Self>| {
            cx.new(|cx| library_pages::LibraryTracksPage::new(section, cx))
        };
        let liked_songs = library_page(LibrarySection::LikedSongs, cx);
        let favorites = library_page(LibrarySection::Favorites, cx);
        let recent = library_page(LibrarySection::Recent, cx);
        let playlists = cx.new(library_pages::PlaylistsPage::new);
        let search = cx.new(|cx| catalog::SearchPage::new(backend.clone(), cx));
        let playlist = cx.new(|cx| catalog::PlaylistPage::new(backend.clone(), cx));
        let artist = cx.new(|cx| catalog::ArtistPage::new(backend.clone(), cx));
        let album = cx.new(|cx| catalog::AlbumPage::new(backend.clone(), cx));
        for subscription in [
            cx.subscribe(&liked_songs, Workspace::handle_page_event),
            cx.subscribe(&favorites, Workspace::handle_page_event),
            cx.subscribe(&recent, Workspace::handle_page_event),
            cx.subscribe(&playlists, Workspace::handle_page_event),
            cx.subscribe(&search, Workspace::handle_page_event),
            cx.subscribe(&playlist, Workspace::handle_page_event),
            cx.subscribe(&artist, Workspace::handle_page_event),
            cx.subscribe(&album, Workspace::handle_page_event),
        ] {
            subscription.detach();
        }
        let onboarding = cx.new(|cx| onboarding::Onboarding::new(window, cx));
        cx.subscribe(
            &onboarding,
            |this, _, event: &onboarding::OnboardingEvent, cx| match event {
                onboarding::OnboardingEvent::Authenticate => this.authenticate(cx),
                onboarding::OnboardingEvent::ChangeSpotifyApp => {
                    this.request_spotify_app_change(cx)
                }
                onboarding::OnboardingEvent::RetryBackend => this.retry_backend(cx),
                onboarding::OnboardingEvent::DismissOverlay => this.cancel_spotify_app_change(cx),
                onboarding::OnboardingEvent::ClearError => {
                    this.last_error = None;
                    cx.notify();
                }
                onboarding::OnboardingEvent::Notice(notice) => {
                    this.action_notice = Some(notice.clone());
                    cx.notify();
                }
            },
        )
        .detach();
        let settings = cx.new(|cx| settings::Settings::new(cx));
        cx.subscribe_in(
            &settings,
            window,
            |this, _, event: &settings::SettingsEvent, window, cx| match event {
                settings::SettingsEvent::RequestAppChange => this.request_spotify_app_change(cx),
                settings::SettingsEvent::SetTheme(preference) => {
                    this.set_theme_preference(*preference, window, cx)
                }
            },
        )
        .detach();
        let sidebar = cx.new(|cx| sidebar::Sidebar::new(preferences.sidebar_collapsed, cx));
        cx.subscribe(
            &sidebar,
            |this, _, event: &sidebar::SidebarEvent, cx| match event {
                sidebar::SidebarEvent::Navigate(route) => this.navigate(*route, cx),
                sidebar::SidebarEvent::OpenPlaylist { playlist, origin } => {
                    this.load_playlist(playlist.clone(), cx);
                    this.open_playlist(*origin, cx);
                }
                sidebar::SidebarEvent::Failed(error) => {
                    this.last_error = Some(error.clone());
                    cx.notify();
                }
            },
        )
        .detach();
        let toolbar = cx.new(|cx| chrome::Toolbar::new(window, cx));
        cx.subscribe(&toolbar, |this, _, event: &ToolbarEvent, cx| match event {
            ToolbarEvent::QueryChanged(query) => {
                let query = query.clone();
                this.search
                    .update(cx, |search, cx| search.set_query(query, cx));
            }
            ToolbarEvent::SubmitSearch => this.submit_search(cx),
            ToolbarEvent::Navigate(route) => this.navigate(*route, cx),
            ToolbarEvent::OpenSettings => this.open_settings(cx),
            ToolbarEvent::Connect => {
                this.session
                    .update(cx, |session, cx| session.set_connecting(cx));
                this.authenticate(cx);
            }
            ToolbarEvent::Logout => this.logout(cx),
            ToolbarEvent::MenuOpened => this.close_queue(cx),
        })
        .detach();
        let player_bar = cx.new(|cx| player_bar::PlayerBar::new(cx));
        cx.subscribe(&player_bar, |this, bar, _: &player_bar::ToggleQueue, cx| {
            if bar.read(cx).queue_open() {
                this.close_account_menu(cx);
            }
            cx.notify();
        })
        .detach();
        let queue_drawer = cx.new(|cx| player_bar::QueueDrawer::new(cx));
        cx.subscribe(&queue_drawer, |this, _, _: &player_bar::CloseQueue, cx| {
            this.close_queue(cx);
        })
        .detach();
        Self {
            router: Router::new(),
            last_error: None,
            action_notice: None,
            radio_request_id: 0,
            pending_radio_request: None,
            player,
            session,
            library,
            liked_songs,
            favorites,
            recent,
            playlists,
            search,
            playlist,
            artist,
            album,
            onboarding,
            settings,
            sidebar,
            toolbar,
            player_bar,
            queue_drawer,
            focus_handle,
            _appearance_subscription: appearance_subscription,
        }
    }

    fn update_system_appearance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if appearance::Appearance::follow_system(window, cx) {
            cx.notify();
        }
    }

    fn set_theme_preference(
        &mut self,
        preference: ThemePreference,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        appearance::Appearance::set_preference(preference, window, cx);
        if let Some(Err(error)) = services::AppServices::set_theme_preference(preference, cx) {
            self.last_error = Some(format!("Could not save appearance preference: {error}"));
        }
        self.close_account_menu(cx);
        cx.notify();
    }

    /// The page for the route the listener is on.
    fn page(&self) -> AnyElement {
        match self.router.route() {
            Route::LikedSongs => self.liked_songs.clone().into_any_element(),
            Route::Favorites => self.favorites.clone().into_any_element(),
            Route::Recent => self.recent.clone().into_any_element(),
            Route::Search => self.search.clone().into_any_element(),
            Route::Playlists => self.playlists.clone().into_any_element(),
            Route::Playlist => self.playlist.clone().into_any_element(),
            Route::Artist => self.artist.clone().into_any_element(),
            Route::Album => self.album.clone().into_any_element(),
            Route::Settings => self.settings.clone().into_any_element(),
        }
    }

    /// The transient banner for things that finished without a page to say so.
    fn action_notice_banner(
        &self,
        palette: CadencePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let message = self.action_notice.clone()?;
        Some(
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
                        components::icon_button(palette, "dismiss-action-notice", "xmark")
                            .size(px(32.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.action_notice = None;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element(),
        )
    }

    fn app_change_confirmation(&self, palette: CadencePalette, cx: &mut Context<Self>) -> Div {
        chrome::spotify_app_change_confirmation(
            palette,
            self.session.read(cx).profile().is_some(),
            cx.listener(|this, _, _, cx| this.cancel_spotify_app_change(cx)),
            cx.listener(|this, _, _, cx| this.confirm_spotify_app_change(cx)),
        )
    }
}

// Render derives the window from memory; all I/O stays behind the backend or
// the image cache.
impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::Appearance::palette(cx);
        let compact_layout = uses_compact_content_layout(f32::from(window.viewport_size().width));
        let app_change_open = self.session.read(cx).app_change_confirmation_open();
        let action_notice = self.action_notice_banner(palette, cx);

        if !self.session.read(cx).is_ready() {
            let error = self.last_error.clone();
            self.onboarding.update(cx, |onboarding, cx| {
                onboarding.set_compact_layout(compact_layout, cx);
                onboarding.show_error(error, cx);
                onboarding.focus_setup_field(window, cx);
            });
            return div()
                .size_full()
                .relative()
                .child(self.onboarding.clone())
                .when_some(action_notice, |root, notice| root.child(notice))
                .when(app_change_open, |root| {
                    root.child(deferred(self.app_change_confirmation(palette, cx)))
                })
                .into_any_element();
        }

        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_compact_layout(compact_layout, cx)
        });
        let route = self.router.route();
        let back_target = self.router.back_target();
        let error = self.last_error.clone();
        self.toolbar.update(cx, |toolbar, cx| {
            toolbar.show(route, back_target, error, cx)
        });

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
            .on_scroll_wheel(cx.listener(|this, _, _, cx| this.close_track_menus(cx)))
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
                    .child(self.sidebar.clone())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .bg(rgb(palette.surface))
                            .child(self.toolbar.clone())
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_hidden()
                                    .child(self.page()),
                            ),
                    )
                    .when(self.player_bar.read(cx).queue_open(), |root| {
                        root.child(self.queue_drawer.clone())
                    }),
            )
            .child(self.player_bar.clone())
            .when_some(action_notice, |root, notice| root.child(notice))
            .when(app_change_open, |root| {
                root.child(deferred(self.app_change_confirmation(palette, cx)))
            })
            .into_any_element()
    }
}
