use super::*;

impl CadenceApp {
    pub(super) fn liked_songs_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let library = self.library.read(cx);
        let tracks = library.liked_tracks().clone();
        let library_loaded = library.loaded();
        let reloading = library.reloading();
        let track_count = tracks.len();
        let list = if library_loaded && tracks.is_empty() {
            components::empty_state(self.palette, "No liked songs").into_any_element()
        } else if tracks.is_empty() {
            components::empty_state(self.palette, "Loading liked songs…").into_any_element()
        } else {
            self.virtual_spotify_track_results("liked-tracks", tracks, cx)
                .into_any_element()
        };
        let detail = components::revalidating_detail(
            if library_loaded {
                format!("{track_count} tracks loaded from Spotify")
            } else {
                "Liked on Spotify".to_owned()
            },
            reloading,
        );
        div()
            .id("liked-songs-page")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .p(px(32.))
            .pt(px(12.))
            .child(self.page_heading("Liked Songs", detail))
            .child(list)
    }

    pub(super) fn favorites_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let library = self.library.read(cx);
        let tracks = library.favorites().clone();
        let content = if library.local_loaded() && tracks.is_empty() {
            components::empty_state(self.palette, "No favorites yet").into_any_element()
        } else if tracks.is_empty() {
            components::empty_state(self.palette, "Loading favorites…").into_any_element()
        } else {
            self.virtual_spotify_track_results("favorite-tracks", tracks, cx)
                .into_any_element()
        };
        div()
            .id("favorites-page")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .p(px(32.))
            .pt(px(12.))
            .child(self.page_heading("Favorites", "Starred in Cadence"))
            .child(content)
    }

    pub(super) fn recent_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let library = self.library.read(cx);
        let tracks = library.recently_played().clone();
        let list = if library.local_loaded() && tracks.is_empty() {
            components::empty_state(self.palette, "No listening history yet").into_any_element()
        } else if tracks.is_empty() {
            components::empty_state(self.palette, "Loading listening history…").into_any_element()
        } else {
            self.virtual_spotify_track_results("recent-tracks", tracks, cx)
                .into_any_element()
        };
        div()
            .id("recent-page")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .p(px(32.))
            .pt(px(12.))
            .child(self.page_heading("Recently played", "Listening history"))
            .child(list)
    }

    pub(super) fn playlists_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let library = self.library.read(cx);
        let spotify_playlists = library.playlists().clone();
        let detail = components::revalidating_detail("Your Spotify playlists", library.reloading());
        let playlists = if library.loaded() && spotify_playlists.is_empty() {
            components::empty_state(self.palette, "No Spotify playlists").into_any_element()
        } else if spotify_playlists.is_empty() {
            components::empty_state(self.palette, "Loading playlists…").into_any_element()
        } else {
            self.virtual_spotify_playlist_results(
                "spotify-playlists",
                spotify_playlists.clone(),
                Route::Playlists,
                cx,
            )
            .into_any_element()
        };
        div()
            .id("playlists-page")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .p(px(32.))
            .pt(px(12.))
            .child(self.page_heading("Playlists", detail))
            .child(playlists)
    }

    pub(super) fn virtual_spotify_playlist_results(
        &mut self,
        id: impl Into<ElementId>,
        playlists: Arc<[model::Playlist]>,
        origin: Route,
        cx: &mut Context<Self>,
    ) -> Div {
        let palette = self.palette;
        let count = playlists.len();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .rounded(px(20.))
            .overflow_hidden()
            .border_1()
            .border_color(rgb(palette.border))
            .child(
                uniform_list(
                    id,
                    count,
                    cx.processor(move |this, range: Range<usize>, _, cx| {
                        range
                            .map(|index| {
                                let playlist = playlists[index].clone();
                                let selected_playlist = playlist.clone();
                                track_row::PlaylistRow::new(
                                    index,
                                    playlist,
                                    this.palette,
                                    this.image_cache.clone(),
                                )
                                .on_open(cx.listener(move |this, _, _, cx| {
                                    this.load_playlist(selected_playlist.clone(), cx);
                                    this.open_playlist(origin, cx);
                                }))
                                .into_any_element()
                            })
                            .collect()
                    }),
                )
                .flex_1()
                .min_h_0(),
            )
    }

    pub(super) fn virtual_spotify_track_results(
        &mut self,
        id: impl Into<ElementId>,
        tracks: Arc<[model::Track]>,
        cx: &mut Context<Self>,
    ) -> Div {
        let palette = self.palette;
        let count = tracks.len();
        let row_tracks = tracks.clone();
        let title_width = if self.compact_layout { 280. } else { 320. };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .rounded(px(20.))
            .overflow_hidden()
            .border_1()
            .border_color(rgb(palette.border))
            .child(
                div()
                    .h(px(40.))
                    .flex_none()
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .bg(rgb(palette.canvas))
                    .text_size(px(11.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(palette.text_muted))
                    .child(div().w(px(44.)).child("#"))
                    .child(div().w(px(title_width)).flex_none().child("Title"))
                    .when(!self.compact_layout, |header| {
                        header.child(div().flex_1().child("Album"))
                    })
                    .child(
                        div()
                            .w(px(36.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(components::icon("star", 12., palette.text_muted)),
                    )
                    .child(
                        div()
                            .w(px(60.))
                            .flex()
                            .items_center()
                            .justify_end()
                            .pr(px(8.))
                            .child("Time"),
                    )
                    .child(div().w(px(36.))),
            )
            .child(
                uniform_list(
                    id,
                    count,
                    cx.processor(move |this, range: Range<usize>, _, cx| {
                        range
                            .filter_map(|index| {
                                row_tracks.get(index).cloned().map(|track| {
                                    this.spotify_track_row(track, index, row_tracks.clone(), cx)
                                })
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .flex_1()
                .min_h_0(),
            )
    }

    pub(super) fn track_action_menu(
        &self,
        actions: TrackActionContext,
        cx: &mut Context<Self>,
    ) -> Div {
        let TrackActionContext {
            track,
            playback_tracks,
            index,
            favorite,
            is_current_track,
            has_playback_context,
        } = actions;
        let palette = self.palette;
        let play_tracks = playback_tracks.clone();
        let next_track = track.clone();
        let queue_track = track.clone();
        let radio_track = track.clone();
        let favorite_track = track.clone();
        let origin = self.route;
        let artist = track
            .artists
            .iter()
            .find(|artist| artist.source_id.is_some())
            .cloned();
        let open_album_id = self
            .album
            .read(cx)
            .reference()
            .and_then(|album| album.source_id.clone());
        let album = track
            .album_ref
            .clone()
            .filter(|album| album.source_id.is_some())
            .filter(|album| self.route != Route::Album || album.source_id != open_album_id);
        let track_url = format!("https://open.spotify.com/track/{}", track.source_id);
        let separator = || {
            div()
                .mx(px(4.))
                .my(px(4.))
                .border_t_1()
                .border_color(rgb(palette.border))
        };

        components::menu_surface(self.palette)
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.track_menu_open = None;
                    cx.notify();
                }),
            )
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .child(
                components::text_menu_item(self.palette, ("track-menu-play", index), "Play now")
                    .when(is_current_track, |item| {
                        item.cursor_default().text_color(rgb(palette.text_muted))
                    })
                    .when(!is_current_track, |item| {
                        item.on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.track_menu_open = None;
                            this.play_context(play_tracks.to_vec(), index, cx);
                        }))
                    }),
            )
            .child(
                components::text_menu_item(self.palette, ("track-menu-next", index), "Play next")
                    .when(!has_playback_context, |item| {
                        item.cursor_default().text_color(rgb(palette.text_muted))
                    })
                    .when(has_playback_context, |item| {
                        item.on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.track_menu_open = None;
                            this.player
                                .update(cx, |player, cx| player.play_next(next_track.clone(), cx));
                            cx.notify();
                        }))
                    }),
            )
            .child(
                components::text_menu_item(
                    self.palette,
                    ("track-menu-queue", index),
                    "Add to queue",
                )
                .when(!has_playback_context, |item| {
                    item.cursor_default().text_color(rgb(palette.text_muted))
                })
                .when(has_playback_context, |item| {
                    item.on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.track_menu_open = None;
                        this.player.update(cx, |player, cx| {
                            player.append_to_queue(queue_track.clone(), cx)
                        });
                        cx.notify();
                    }))
                }),
            )
            .child(
                components::text_menu_item(
                    self.palette,
                    ("track-menu-radio", index),
                    "Start track radio",
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.track_menu_open = None;
                    this.action_notice = Some("Starting track radio…".to_owned());
                    let request_id = next_request_id(&mut this.radio_request_id);
                    this.pending_radio_request = Some(request_id);
                    if !this.player.update(cx, |player, cx| {
                        player.start_radio(request_id, radio_track.clone(), cx)
                    }) {
                        this.pending_radio_request = None;
                        this.action_notice = Some("Unable to start track radio".to_owned());
                    }
                    cx.notify();
                })),
            )
            .child(separator())
            .child(
                components::text_menu_item(
                    self.palette,
                    ("track-menu-favorite", index),
                    if favorite {
                        "Remove from favorites"
                    } else {
                        "Add to favorites"
                    },
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.track_menu_open = None;
                    this.library.update(cx, |library, cx| {
                        library.set_favorite(favorite_track.clone(), !favorite, cx)
                    });
                    cx.notify();
                })),
            )
            .child(separator())
            .when_some(artist, |menu, artist| {
                menu.child(
                    components::text_menu_item(
                        self.palette,
                        ("track-menu-artist", index),
                        "Go to artist",
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.track_menu_open = None;
                        this.open_artist(artist.clone(), origin, cx);
                    })),
                )
            })
            .when_some(album, |menu, album| {
                menu.child(
                    components::text_menu_item(
                        self.palette,
                        ("track-menu-album", index),
                        "Go to album",
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.track_menu_open = None;
                        this.open_album(album.clone(), origin, cx);
                    })),
                )
            })
            .child(
                components::text_menu_item(
                    self.palette,
                    ("track-menu-spotify", index),
                    "Open track in Spotify",
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.track_menu_open = None;
                    cx.open_url(&track_url);
                    cx.notify();
                })),
            )
    }

    pub(super) fn spotify_track_row(
        &mut self,
        track: model::Track,
        index: usize,
        playback_tracks: Arc<[model::Track]>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let is_current_track = self.player.read(cx).is_current_track(&track);
        let favorite = self.library.read(cx).is_favorite(&track);
        let has_playback_context = self.player.read(cx).now_playing().is_some();
        let menu_key = format!("{}:{index}", track.source_id);
        let menu_open = self.track_menu_open.as_deref() == Some(menu_key.as_str());
        let menu = menu_open.then(|| {
            self.track_action_menu(
                TrackActionContext {
                    track: track.clone(),
                    playback_tracks: playback_tracks.clone(),
                    index,
                    favorite,
                    is_current_track,
                    has_playback_context,
                },
                cx,
            )
            .into_any_element()
        });
        let favorite_track = track.clone();
        track_row::TrackRow::new(index, track, self.palette, self.image_cache.clone())
            .compact(self.compact_layout)
            .current(is_current_track)
            .favorite(favorite)
            .menu(menu_open, menu)
            .on_play(cx.listener(move |this, _, _, cx| {
                this.play_context(playback_tracks.to_vec(), index, cx);
            }))
            .on_favorite(cx.listener(move |this, _, _, cx| {
                this.library.update(cx, |library, cx| {
                    library.set_favorite(favorite_track.clone(), !favorite, cx)
                });
            }))
            .on_toggle_menu(cx.listener(move |this, _, _, cx| {
                if menu_open {
                    this.track_menu_open = None;
                } else {
                    this.track_menu_open = Some(menu_key.clone());
                    this.account_menu_open = false;
                    this.close_queue(cx);
                }
                cx.notify();
            }))
            .into_any_element()
    }
}
