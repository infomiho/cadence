use super::*;

use page::PageEvent;

/// A virtualized table of tracks with a per-row action menu.
///
/// The list owns which row's menu is open, and a click landing anywhere else
/// dismisses it. Dismissal that no click drives -- a keyboard route change, a
/// scroll outside the table -- is the workspace's to trigger, via
/// `close_menu`, because the menu outlives the page going off screen.
pub(super) struct TrackList {
    /// Set by `show`, which always runs before the list is first painted.
    id: Option<ElementId>,
    tracks: Arc<[model::Track]>,
    /// The row whose action menu is open, keyed by source ID and row index so
    /// the same track appearing twice opens only the row that was clicked.
    menu_open: Option<String>,
    /// Album already on screen, whose "Go to album" entry would go nowhere.
    current_album_id: Option<String>,
    library: Entity<library::Library>,
    player: Entity<player::Player>,
    image_cache: Entity<image_cache::BoundedImageCache>,
}

impl EventEmitter<PageEvent> for TrackList {}

impl TrackList {
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            id: None,
            tracks: Arc::default(),
            menu_open: None,
            current_album_id: None,
            library: services::AppServices::library(cx),
            player: services::AppServices::player(cx),
            image_cache: services::AppServices::image_cache(cx),
        }
    }

    /// Shows `tracks` under `id`, which pages vary per playlist or album so
    /// that opening a different one starts back at the top of the list.
    ///
    /// Pages call this from `render`, so the early return below is what keeps
    /// the notify cycle finite: callers must pass a stored `Arc` clone, not a
    /// slice rebuilt every frame, or every render schedules another one.
    pub(super) fn show(
        &mut self,
        id: impl Into<ElementId>,
        tracks: Arc<[model::Track]>,
        cx: &mut Context<Self>,
    ) {
        let id = Some(id.into());
        if self.id == id && Arc::ptr_eq(&self.tracks, &tracks) {
            return;
        }
        self.id = id;
        self.tracks = tracks;
        self.menu_open = None;
        cx.notify();
    }

    /// Suppresses the "Go to album" entry for the album the page is showing.
    pub(super) fn set_current_album_id(
        &mut self,
        source_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.current_album_id != source_id {
            self.current_album_id = source_id;
            cx.notify();
        }
    }

    pub(super) fn close_menu(&mut self, cx: &mut Context<Self>) {
        if self.menu_open.take().is_some() {
            cx.notify();
        }
    }

    fn row(&mut self, index: usize, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        let Some(track) = self.tracks.get(index).cloned() else {
            return div().into_any_element();
        };
        let palette = appearance::Appearance::palette(cx);
        let is_current_track = self.player.read(cx).is_current_track(&track);
        let favorite = self.library.read(cx).is_favorite(&track);
        let menu_key = format!("{}:{index}", track.source_id);
        let menu_open = self.menu_open.as_deref() == Some(menu_key.as_str());
        let menu = menu_open
            .then(|| self.action_menu(&track, index, favorite, is_current_track, cx))
            .map(IntoElement::into_any_element);
        let favorite_track = track.clone();
        track_row::TrackRow::new(index, track, palette, self.image_cache.clone())
            .compact(compact)
            .current(is_current_track)
            .favorite(favorite)
            .menu(menu_open, menu)
            .on_play(cx.listener(move |this, _, _, cx| this.play_from(index, cx)))
            .on_favorite(cx.listener(move |this, _, _, cx| {
                this.library.update(cx, |library, cx| {
                    library.set_favorite(favorite_track.clone(), !favorite, cx)
                });
            }))
            .on_toggle_menu(cx.listener(move |this, _, _, cx| {
                this.menu_open = (!menu_open).then(|| menu_key.clone());
                cx.notify();
            }))
            .into_any_element()
    }

    fn play_from(&mut self, index: usize, cx: &mut Context<Self>) {
        let tracks = self.tracks.to_vec();
        self.player
            .update(cx, |player, cx| player.play_context(tracks, index, cx));
    }

    fn action_menu(
        &self,
        track: &model::Track,
        index: usize,
        favorite: bool,
        is_current_track: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let palette = appearance::Appearance::palette(cx);
        let has_playback_context = self.player.read(cx).now_playing().is_some();
        let next_track = track.clone();
        let queue_track = track.clone();
        let radio_track = track.clone();
        let favorite_track = track.clone();
        let artist = track
            .artists
            .iter()
            .find(|artist| artist.source_id.is_some())
            .cloned();
        let album = track
            .album_ref
            .clone()
            .filter(|album| album.source_id.is_some())
            .filter(|album| album.source_id != self.current_album_id);
        let track_url = format!("https://open.spotify.com/track/{}", track.source_id);
        let separator = || {
            div()
                .mx(px(4.))
                .my(px(4.))
                .border_t_1()
                .border_color(rgb(palette.border))
        };

        components::menu_surface(palette)
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| this.close_menu(cx)),
            )
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .child(
                components::text_menu_item(palette, ("track-menu-play", index), "Play now")
                    .when(is_current_track, |item| {
                        item.cursor_default().text_color(rgb(palette.text_muted))
                    })
                    .when(!is_current_track, |item| {
                        item.on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.menu_open = None;
                            this.play_from(index, cx);
                        }))
                    }),
            )
            .child(
                components::text_menu_item(palette, ("track-menu-next", index), "Play next")
                    .when(!has_playback_context, |item| {
                        item.cursor_default().text_color(rgb(palette.text_muted))
                    })
                    .when(has_playback_context, |item| {
                        item.on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.menu_open = None;
                            this.player
                                .update(cx, |player, cx| player.play_next(next_track.clone(), cx));
                            cx.notify();
                        }))
                    }),
            )
            .child(
                components::text_menu_item(palette, ("track-menu-queue", index), "Add to queue")
                    .when(!has_playback_context, |item| {
                        item.cursor_default().text_color(rgb(palette.text_muted))
                    })
                    .when(has_playback_context, |item| {
                        item.on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.menu_open = None;
                            this.player.update(cx, |player, cx| {
                                player.append_to_queue(queue_track.clone(), cx)
                            });
                            cx.notify();
                        }))
                    }),
            )
            .child(
                components::text_menu_item(
                    palette,
                    ("track-menu-radio", index),
                    "Start track radio",
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.menu_open = None;
                    cx.emit(PageEvent::StartRadio(radio_track.clone()));
                    cx.notify();
                })),
            )
            .child(separator())
            .child(
                components::text_menu_item(
                    palette,
                    ("track-menu-favorite", index),
                    if favorite {
                        "Remove from favorites"
                    } else {
                        "Add to favorites"
                    },
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.menu_open = None;
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
                        palette,
                        ("track-menu-artist", index),
                        "Go to artist",
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.menu_open = None;
                        cx.emit(PageEvent::OpenArtist(artist.clone()));
                    })),
                )
            })
            .when_some(album, |menu, album| {
                menu.child(
                    components::text_menu_item(palette, ("track-menu-album", index), "Go to album")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.menu_open = None;
                            cx.emit(PageEvent::OpenAlbum(album.clone()));
                        })),
                )
            })
            .child(
                components::text_menu_item(
                    palette,
                    ("track-menu-spotify", index),
                    "Open track in Spotify",
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.menu_open = None;
                    cx.open_url(&track_url);
                    cx.notify();
                })),
            )
    }
}

impl Render for TrackList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::Appearance::palette(cx);
        let compact = uses_compact_content_layout(f32::from(window.viewport_size().width));
        div()
            .id("track-list")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .rounded(px(20.))
            .overflow_hidden()
            .border_1()
            .border_color(rgb(palette.border))
            .child(track_row::track_list_header(palette, compact))
            .child(
                uniform_list(
                    self.id
                        .clone()
                        .expect("show sets the id before first paint"),
                    self.tracks.len(),
                    cx.processor(move |this, range: Range<usize>, _, cx| {
                        range.map(|index| this.row(index, compact, cx)).collect()
                    }),
                )
                .flex_1()
                .min_h_0(),
            )
    }
}

/// A virtualized table of playlists.
pub(super) struct PlaylistList {
    /// Set by `show`, which always runs before the list is first painted.
    id: Option<ElementId>,
    playlists: Arc<[model::Playlist]>,
    image_cache: Entity<image_cache::BoundedImageCache>,
}

impl EventEmitter<PageEvent> for PlaylistList {}

impl PlaylistList {
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            id: None,
            playlists: Arc::default(),
            image_cache: services::AppServices::image_cache(cx),
        }
    }

    /// Shows `playlists` under `id`, which pages vary per content so that a
    /// new set starts back at the top. Same render-time contract as
    /// [`TrackList::show`]: pass a stored `Arc` clone, not a fresh slice.
    pub(super) fn show(
        &mut self,
        id: impl Into<ElementId>,
        playlists: Arc<[model::Playlist]>,
        cx: &mut Context<Self>,
    ) {
        let id = Some(id.into());
        if self.id == id && Arc::ptr_eq(&self.playlists, &playlists) {
            return;
        }
        self.id = id;
        self.playlists = playlists;
        cx.notify();
    }
}

impl Render for PlaylistList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::Appearance::palette(cx);
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
                    self.id
                        .clone()
                        .expect("show sets the id before first paint"),
                    self.playlists.len(),
                    cx.processor(move |this, range: Range<usize>, _, cx| {
                        let playlists = this.playlists.clone();
                        range
                            .filter_map(|index| {
                                playlists.get(index).cloned().map(|playlist| {
                                    let selected = playlist.clone();
                                    track_row::PlaylistRow::new(
                                        index,
                                        playlist,
                                        palette,
                                        this.image_cache.clone(),
                                    )
                                    .on_open(cx.listener(move |_, _, _, cx| {
                                        cx.emit(PageEvent::OpenPlaylist(selected.clone()));
                                    }))
                                    .into_any_element()
                                })
                            })
                            .collect()
                    }),
                )
                .flex_1()
                .min_h_0(),
            )
    }
}
