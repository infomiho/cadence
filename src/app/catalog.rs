use super::*;

impl CadenceApp {
    pub(super) fn search_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let results = if self.search_error.is_some() {
            self.empty_state("Unable to search Spotify")
                .into_any_element()
        } else {
            match self.search_kind {
                SearchKind::Tracks if !self.search_results.is_empty() => self
                    .virtual_spotify_track_results("search-tracks", self.search_results.clone(), cx)
                    .into_any_element(),
                SearchKind::Tracks if self.search_loaded => {
                    self.empty_state("No tracks found").into_any_element()
                }
                SearchKind::Tracks if self.searching => {
                    self.empty_state("Searching Spotify…").into_any_element()
                }
                SearchKind::Tracks => self
                    .empty_state("Press Return to search")
                    .into_any_element(),
                SearchKind::Playlists if !self.search_playlists.is_empty() => self
                    .virtual_spotify_playlist_results(
                        "search-playlists",
                        self.search_playlists.clone(),
                        Route::Search,
                        cx,
                    )
                    .into_any_element(),
                SearchKind::Playlists if self.search_loaded => {
                    self.empty_state("No playlists found").into_any_element()
                }
                SearchKind::Playlists if self.searching => {
                    self.empty_state("Searching Spotify…").into_any_element()
                }
                SearchKind::Playlists => self
                    .empty_state("Press Return to search")
                    .into_any_element(),
            }
        };

        div()
            .id("search-page")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .p(px(32.))
            .pt(px(12.))
            .child(self.page_heading(
                "Search results",
                format!("Results for {}", self.search_query),
            ))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .mb(px(20.))
                    .child(
                        self.pill(
                            "tab-tracks",
                            "Tracks",
                            self.search_kind == SearchKind::Tracks,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.search_kind = SearchKind::Tracks;
                            cx.notify();
                        })),
                    )
                    .child(
                        self.pill(
                            "tab-playlists",
                            "Playlists",
                            self.search_kind == SearchKind::Playlists,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.search_kind = SearchKind::Playlists;
                            cx.notify();
                        })),
                    ),
            )
            .child(results)
    }

    pub(super) fn playlist_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let selected_playlist = self.selected_spotify_playlist.clone();
        let playlist_artwork = selected_playlist
            .as_ref()
            .and_then(|playlist| playlist.artwork_url.clone());
        let playlist_pinned = selected_playlist.as_ref().is_some_and(|playlist| {
            self.pinned_playlists.iter().any(|pinned| {
                pinned.provider == playlist.provider && pinned.source_id == playlist.source_id
            })
        });
        let pin_icon = if playlist_pinned { "pin.fill" } else { "pin" };
        let (playlist_name, playlist_detail) = self
            .selected_spotify_playlist
            .as_ref()
            .map(|playlist| {
                (
                    SharedString::from(playlist.name.clone()),
                    SharedString::from(format!(
                        "Spotify playlist · {} tracks",
                        playlist.track_count
                    )),
                )
            })
            .unwrap_or_else(|| {
                (
                    SharedString::from("Playlist"),
                    SharedString::from("Spotify playlist"),
                )
            });
        let first_track = self.playlist_tracks.first().cloned();
        let playlist_context = self.playlist_tracks.clone();
        let tracks = self.playlist_tracks.clone();
        let playlist_list_id = self.selected_spotify_playlist.as_ref().map(|playlist| {
            (
                ElementId::from("playlist-tracks"),
                playlist.source_id.clone(),
            )
        });
        let list = if let Some(error) = &self.playlist_error {
            self.empty_state(format!("Unable to load playlist: {error}"))
                .into_any_element()
        } else if self.selected_spotify_playlist.is_some() && !tracks.is_empty() {
            self.virtual_spotify_track_results(
                playlist_list_id.expect("selected playlist has a list ID"),
                tracks,
                cx,
            )
            .into_any_element()
        } else if self.selected_spotify_playlist.is_some() && self.playlist_loaded {
            self.empty_state("This playlist is empty")
                .into_any_element()
        } else if self.selected_spotify_playlist.is_some() {
            self.empty_state("Loading playlist…").into_any_element()
        } else {
            self.empty_state("No playlist selected").into_any_element()
        };

        div()
            .id("playlist-page")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .p(px(32.))
            .pt(px(8.))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(28.))
                    .mb(px(24.))
                    .child(self.artwork(playlist_artwork.as_deref(), 176., 28., "music.note.list"))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(40.))
                                    .line_height(px(44.))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(palette.text_primary))
                                    .child(playlist_name),
                            )
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .text_color(rgb(palette.text_muted))
                                    .child(playlist_detail),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.))
                                    .mt(px(8.))
                                    .child(self.pill("playlist-play", "Play", true).on_click(
                                        cx.listener(move |this, _, _, cx| {
                                            if first_track.is_some()
                                                && this.send_backend(BackendCommand::PlayContext {
                                                    tracks: playlist_context.to_vec(),
                                                    index: 0,
                                                })
                                            {
                                                this.position_ms = 0;
                                                this.playback_loading = true;
                                            }
                                            cx.notify();
                                        }),
                                    ))
                                    .child(
                                        self.icon_button("playlist-pin", pin_icon)
                                            .bg(rgb(if playlist_pinned {
                                                palette.selection
                                            } else {
                                                palette.control
                                            }))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                if let Some(playlist) = selected_playlist.clone() {
                                                    this.send_backend(
                                                        BackendCommand::SetPlaylistPinned {
                                                            playlist,
                                                            pinned: !playlist_pinned,
                                                        },
                                                    );
                                                }
                                                cx.notify();
                                            })),
                                    ),
                            ),
                    ),
            )
            .child(list)
    }

    pub(super) fn artist_album_card(
        &self,
        album: model::Album,
        index: usize,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let palette = self.palette;
        let album_ref = model::AlbumRef {
            name: album.name.clone(),
            source_id: Some(album.source_id.clone()),
            spotify_uri: album.spotify_uri.clone(),
            artwork_url: album.artwork_url.clone(),
        };
        let detail = album
            .release_date
            .as_deref()
            .and_then(|date| date.get(..4))
            .unwrap_or("Release")
            .to_owned();

        self.button(("artist-album", index))
            .flex_1()
            .min_w_0()
            .h(px(244.))
            .p(px(10.))
            .flex_col()
            .items_center()
            .justify_start()
            .rounded(px(16.))
            .hover(|style| style.bg(rgb(palette.surface_hover)))
            .child(
                div()
                    .w(px(152.))
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(10.))
                    .child(self.artwork(album.artwork_url.as_deref(), 152., 14., "music.note"))
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(palette.text_primary))
                            .child(album.name),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(palette.text_muted))
                            .child(detail),
                    ),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_album(album_ref.clone(), Route::Artist, cx);
            }))
            .into_any_element()
    }

    pub(super) fn artist_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let artist_name: SharedString = self
            .selected_artist
            .as_ref()
            .map(|artist| artist.name.clone())
            .or_else(|| {
                self.selected_artist_ref
                    .as_ref()
                    .map(|artist| artist.name.clone())
            })
            .unwrap_or_else(|| "Artist".to_owned())
            .into();
        let artwork_url = self
            .selected_artist
            .as_ref()
            .and_then(|artist| artist.artwork_url.clone());
        let album_count = self.artist_albums.len();
        let artist_detail = if self.artist_loaded && self.artist_error.is_none() {
            format!("Spotify artist · {album_count} releases")
        } else {
            "Spotify artist".to_owned()
        };
        let section = self.artist_section;
        let content = if let Some(error) = &self.artist_error {
            self.empty_state(format!("Unable to load artist: {error}"))
                .into_any_element()
        } else if !self.artist_loaded {
            self.empty_state("Loading artist…").into_any_element()
        } else if section == ArtistSection::Popular && !self.artist_tracks.is_empty() {
            let source_id = self
                .selected_artist_ref
                .as_ref()
                .and_then(|artist| artist.source_id.clone())
                .unwrap_or_default();
            self.virtual_spotify_track_results(
                (ElementId::from("artist-popular"), source_id),
                self.artist_tracks.clone(),
                cx,
            )
            .into_any_element()
        } else if section == ArtistSection::Popular {
            self.empty_state("No popular tracks available")
                .into_any_element()
        } else if !self.artist_albums.is_empty() {
            let albums = self.artist_albums.clone();
            let columns = if self.compact_layout { 3 } else { 4 };
            let row_count = albums.len().div_ceil(columns);
            let row_albums = albums.clone();
            uniform_list(
                "artist-discography",
                row_count,
                cx.processor(move |this, range: Range<usize>, _, cx| {
                    range
                        .map(|row| {
                            let start = row * columns;
                            let end = (start + columns).min(row_albums.len());
                            let mut cards =
                                div().h(px(256.)).w_full().flex().items_start().gap(px(12.));
                            for index in start..end {
                                cards = cards.child(this.artist_album_card(
                                    row_albums[index].clone(),
                                    index,
                                    cx,
                                ));
                            }
                            for _ in end..start + columns {
                                cards = cards.child(div().flex_1().min_w_0());
                            }
                            cards
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .flex_1()
            .min_h_0()
            .into_any_element()
        } else {
            self.empty_state("No releases available").into_any_element()
        };

        div()
            .id("artist-page")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .p(px(32.))
            .pt(px(8.))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(28.))
                    .mb(px(24.))
                    .child(self.artwork(artwork_url.as_deref(), 144., 72., "person.fill"))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(40.))
                                    .line_height(px(44.))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(palette.text_primary))
                                    .child(artist_name),
                            )
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .text_color(rgb(palette.text_muted))
                                    .child(artist_detail),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .gap(px(8.))
                    .mb(px(16.))
                    .child(
                        self.pill(
                            "artist-tab-popular",
                            "Popular",
                            section == ArtistSection::Popular,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.artist_section = ArtistSection::Popular;
                            this.track_menu_open = None;
                            cx.notify();
                        })),
                    )
                    .child(
                        self.pill(
                            "artist-tab-discography",
                            "Discography",
                            section == ArtistSection::Discography,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.artist_section = ArtistSection::Discography;
                            this.track_menu_open = None;
                            cx.notify();
                        })),
                    ),
            )
            .child(content)
    }

    pub(super) fn album_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let album_name: SharedString = self
            .selected_album
            .as_ref()
            .map(|album| album.name.clone())
            .or_else(|| {
                self.selected_album_ref
                    .as_ref()
                    .map(|album| album.name.clone())
            })
            .unwrap_or_else(|| "Album".to_owned())
            .into();
        let artwork_url = self
            .selected_album
            .as_ref()
            .and_then(|album| album.artwork_url.clone())
            .or_else(|| {
                self.selected_album_ref
                    .as_ref()
                    .and_then(|album| album.artwork_url.clone())
            });
        let album_detail = self.selected_album.as_ref().map_or_else(
            || "Spotify album".to_owned(),
            |album| {
                let artists = album
                    .artists
                    .iter()
                    .map(|artist| artist.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                let year = album.release_date.as_deref().and_then(|date| date.get(..4));
                let mut details = vec![artists];
                if let Some(year) = year {
                    details.push(year.to_owned());
                }
                if let Some(track_count) = album.track_count {
                    details.push(format!("{track_count} tracks"));
                }
                details.join(" · ")
            },
        );
        let tracks = self.album_tracks.clone();
        let playback_tracks = tracks.clone();
        let list = if let Some(error) = &self.album_error {
            self.empty_state(format!("Unable to load album: {error}"))
                .into_any_element()
        } else if !tracks.is_empty() {
            let source_id = self
                .selected_album_ref
                .as_ref()
                .and_then(|album| album.source_id.clone())
                .unwrap_or_default();
            self.virtual_spotify_track_results(
                (ElementId::from("album-tracks"), source_id),
                tracks,
                cx,
            )
            .into_any_element()
        } else if self.album_loaded {
            self.empty_state("This album has no playable tracks")
                .into_any_element()
        } else {
            self.empty_state("Loading album…").into_any_element()
        };

        div()
            .id("album-page")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .p(px(32.))
            .pt(px(8.))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(28.))
                    .mb(px(24.))
                    .child(self.artwork(artwork_url.as_deref(), 176., 28., "music.note"))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(40.))
                                    .line_height(px(44.))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(palette.text_primary))
                                    .child(album_name),
                            )
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .text_color(rgb(palette.text_muted))
                                    .child(album_detail),
                            )
                            .child(self.pill("album-play", "Play", true).on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if !playback_tracks.is_empty()
                                        && this.send_backend(BackendCommand::PlayContext {
                                            tracks: playback_tracks.to_vec(),
                                            index: 0,
                                        })
                                    {
                                        this.position_ms = 0;
                                        this.playback_loading = true;
                                    }
                                    cx.notify();
                                },
                            ))),
                    ),
            )
            .child(list)
    }
}
