use super::*;

use catalog::{AlbumPage, ArtistPage, PlaylistPage, SearchPage};
use page::PageEvent;

impl Render for SearchPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::Appearance::palette(cx);
        let kind = self.kind();
        let query = self.query().to_owned();
        let tracks = self.tracks().clone();
        let playlists = self.playlists().clone();
        let searching = self.searching();
        let loaded = self.loaded();
        let results = if self.error().is_some() {
            components::empty_state(palette, "Unable to search Spotify").into_any_element()
        } else if kind == SearchKind::Tracks && !tracks.is_empty() {
            self.track_list
                .update(cx, |list, cx| list.show("search-tracks", tracks, cx));
            self.track_list.clone().into_any_element()
        } else if kind == SearchKind::Playlists && !playlists.is_empty() {
            self.playlist_list
                .update(cx, |list, cx| list.show(playlists, cx));
            self.playlist_list.clone().into_any_element()
        } else {
            let message = match (loaded, searching) {
                (true, _) if kind == SearchKind::Tracks => "No tracks found",
                (true, _) => "No playlists found",
                (_, true) => "Searching Spotify…",
                _ => "Press Return to search",
            };
            components::empty_state(palette, message).into_any_element()
        };

        components::page("search-page")
            .pt(px(12.))
            .overflow_hidden()
            .child(components::page_heading(
                palette,
                "Search results",
                format!("Results for {query}"),
            ))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .mb(px(20.))
                    .child(
                        components::pill(
                            palette,
                            "tab-tracks",
                            "Tracks",
                            kind == SearchKind::Tracks,
                        )
                        .on_click(
                            cx.listener(|this, _, _, cx| this.set_kind(SearchKind::Tracks, cx)),
                        ),
                    )
                    .child(
                        components::pill(
                            palette,
                            "tab-playlists",
                            "Playlists",
                            kind == SearchKind::Playlists,
                        )
                        .on_click(
                            cx.listener(|this, _, _, cx| this.set_kind(SearchKind::Playlists, cx)),
                        ),
                    ),
            )
            .child(results)
    }
}

impl Render for PlaylistPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::Appearance::palette(cx);
        let selected = self.selected().cloned();
        let tracks = self.tracks().clone();
        let loaded = self.loaded();
        let error = self.error().cloned();
        let pinned = selected.as_ref().is_some_and(|playlist| {
            self.library
                .read(cx)
                .pinned_playlists()
                .iter()
                .any(|candidate| {
                    candidate.provider == playlist.provider
                        && candidate.source_id == playlist.source_id
                })
        });
        let (name, detail) = selected.as_ref().map_or_else(
            || ("Playlist".to_owned(), "Spotify playlist".to_owned()),
            |playlist| {
                (
                    playlist.name.clone(),
                    format!("Spotify playlist · {} tracks", playlist.track_count),
                )
            },
        );
        let artwork_url = selected
            .as_ref()
            .and_then(|playlist| playlist.artwork_url.clone());
        let list = if let Some(error) = &error {
            components::empty_state(palette, format!("Unable to load playlist: {error}"))
                .into_any_element()
        } else if let Some(playlist) = selected.as_ref().filter(|_| !tracks.is_empty()) {
            let list_id = (
                ElementId::from("playlist-tracks"),
                playlist.source_id.clone(),
            );
            self.track_list
                .update(cx, |list, cx| list.show(list_id, tracks.clone(), cx));
            self.track_list.clone().into_any_element()
        } else if selected.is_none() {
            components::empty_state(palette, "No playlist selected").into_any_element()
        } else if loaded {
            components::empty_state(palette, "This playlist is empty").into_any_element()
        } else {
            components::empty_state(palette, "Loading playlist…").into_any_element()
        };
        let playback_tracks = tracks.clone();
        let pinned_playlist = selected;

        components::page("playlist-page")
            .pt(px(8.))
            .child(
                page_header()
                    .child(components::artwork(
                        palette,
                        &self.image_cache,
                        artwork_url.as_deref(),
                        176.,
                        28.,
                        "music.note.list",
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(8.))
                            .child(page_title(palette, name))
                            .child(page_detail(palette, detail))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.))
                                    .mt(px(8.))
                                    .child(
                                        components::pill(palette, "playlist-play", "Play", true)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.play(&playback_tracks, cx);
                                            })),
                                    )
                                    .child(
                                        components::icon_button(
                                            palette,
                                            "playlist-pin",
                                            if pinned { "pin.fill" } else { "pin" },
                                        )
                                        .bg(rgb(if pinned {
                                            palette.selection
                                        } else {
                                            palette.control
                                        }))
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                if let Some(playlist) = pinned_playlist.clone() {
                                                    this.library.update(cx, |library, cx| {
                                                        library.set_playlist_pinned(
                                                            playlist, !pinned, cx,
                                                        )
                                                    });
                                                }
                                                cx.notify();
                                            }),
                                        ),
                                    ),
                            ),
                    ),
            )
            .child(list)
    }
}

impl Render for ArtistPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::Appearance::palette(cx);
        let name = self
            .artist()
            .map(|artist| artist.name.clone())
            .or_else(|| self.reference().map(|artist| artist.name.clone()))
            .unwrap_or_else(|| "Artist".to_owned());
        let artwork_url = self.artist().and_then(|artist| artist.artwork_url.clone());
        let source_id = self
            .reference()
            .and_then(|artist| artist.source_id.clone())
            .unwrap_or_default();
        let detail = if self.loaded() && self.error().is_none() {
            format!("Spotify artist · {} releases", self.albums().len())
        } else {
            "Spotify artist".to_owned()
        };
        let section = self.section();
        let tracks = self.tracks().clone();
        let albums = self.albums().clone();
        let content = if let Some(error) = self.error() {
            components::empty_state(palette, format!("Unable to load artist: {error}"))
                .into_any_element()
        } else if !self.loaded() {
            components::empty_state(palette, "Loading artist…").into_any_element()
        } else if section == ArtistSection::Popular && !tracks.is_empty() {
            let list_id = (ElementId::from("artist-popular"), source_id);
            self.track_list
                .update(cx, |list, cx| list.show(list_id, tracks, cx));
            self.track_list.clone().into_any_element()
        } else if section == ArtistSection::Popular {
            components::empty_state(palette, "No popular tracks available").into_any_element()
        } else if albums.is_empty() {
            components::empty_state(palette, "No releases available").into_any_element()
        } else {
            self.discography(albums, window, cx).into_any_element()
        };

        components::page("artist-page")
            .pt(px(8.))
            .child(
                page_header()
                    .child(components::artwork(
                        palette,
                        &self.image_cache,
                        artwork_url.as_deref(),
                        144.,
                        72.,
                        "person.fill",
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(page_title(palette, name))
                            .child(page_detail(palette, detail)),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .gap(px(8.))
                    .mb(px(16.))
                    .child(
                        components::pill(
                            palette,
                            "artist-tab-popular",
                            "Popular",
                            section == ArtistSection::Popular,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_section(ArtistSection::Popular, cx)
                        })),
                    )
                    .child(
                        components::pill(
                            palette,
                            "artist-tab-discography",
                            "Discography",
                            section == ArtistSection::Discography,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_section(ArtistSection::Discography, cx)
                        })),
                    ),
            )
            .child(content)
    }
}

impl ArtistPage {
    /// The release grid, laid out as rows of fixed-width cards so that the
    /// whole discography can be virtualized like a list.
    fn discography(
        &mut self,
        albums: Arc<[model::Album]>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let columns = if uses_compact_content_layout(f32::from(window.viewport_size().width)) {
            3
        } else {
            4
        };
        let row_count = albums.len().div_ceil(columns);
        uniform_list(
            "artist-discography",
            row_count,
            cx.processor(move |this, range: Range<usize>, _, cx| {
                range
                    .map(|row| {
                        let start = row * columns;
                        let end = (start + columns).min(albums.len());
                        let mut cards =
                            div().h(px(256.)).w_full().flex().items_start().gap(px(12.));
                        for index in start..end {
                            cards = cards.child(this.album_card(&albums[index], index, cx));
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
    }

    fn album_card(&self, album: &model::Album, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let palette = appearance::Appearance::palette(cx);
        let reference = model::AlbumRef {
            name: album.name.clone(),
            source_id: Some(album.source_id.clone()),
            spotify_uri: album.spotify_uri.clone(),
            artwork_url: album.artwork_url.clone(),
        };
        let year = album
            .release_date
            .as_deref()
            .and_then(|date| date.get(..4))
            .unwrap_or("Release")
            .to_owned();

        components::button(palette, ("artist-album", index))
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
                    .child(components::artwork(
                        palette,
                        &self.image_cache,
                        album.artwork_url.as_deref(),
                        152.,
                        14.,
                        "music.note",
                    ))
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(palette.text_primary))
                            .child(album.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(palette.text_muted))
                            .child(year),
                    ),
            )
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.emit(PageEvent::OpenAlbum(reference.clone()));
            }))
            .into_any_element()
    }
}

impl Render for AlbumPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::Appearance::palette(cx);
        let name = self
            .album()
            .map(|album| album.name.clone())
            .or_else(|| self.reference().map(|album| album.name.clone()))
            .unwrap_or_else(|| "Album".to_owned());
        let artwork_url = self
            .album()
            .and_then(|album| album.artwork_url.clone())
            .or_else(|| self.reference().and_then(|album| album.artwork_url.clone()));
        let source_id = self.reference().and_then(|album| album.source_id.clone());
        let detail = self.album().map_or_else(
            || "Spotify album".to_owned(),
            |album| {
                let artists = album
                    .artists
                    .iter()
                    .map(|artist| artist.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                let mut details = vec![artists];
                if let Some(year) = album.release_date.as_deref().and_then(|date| date.get(..4)) {
                    details.push(year.to_owned());
                }
                if let Some(track_count) = album.track_count {
                    details.push(format!("{track_count} tracks"));
                }
                details.join(" · ")
            },
        );
        let tracks = self.tracks().clone();
        let loaded = self.loaded();
        let list = if let Some(error) = self.error() {
            components::empty_state(palette, format!("Unable to load album: {error}"))
                .into_any_element()
        } else if !tracks.is_empty() {
            let list_id = (
                ElementId::from("album-tracks"),
                source_id.clone().unwrap_or_default(),
            );
            self.track_list.update(cx, |list, cx| {
                list.set_current_album_id(source_id, cx);
                list.show(list_id, tracks.clone(), cx);
            });
            self.track_list.clone().into_any_element()
        } else if loaded {
            components::empty_state(palette, "This album has no playable tracks").into_any_element()
        } else {
            components::empty_state(palette, "Loading album…").into_any_element()
        };
        let playback_tracks = tracks;

        components::page("album-page")
            .pt(px(8.))
            .child(
                page_header()
                    .child(components::artwork(
                        palette,
                        &self.image_cache,
                        artwork_url.as_deref(),
                        176.,
                        28.,
                        "music.note",
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(8.))
                            .child(page_title(palette, name))
                            .child(page_detail(palette, detail))
                            .child(
                                components::pill(palette, "album-play", "Play", true).on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        this.play(&playback_tracks, cx);
                                    }),
                                ),
                            ),
                    ),
            )
            .child(list)
    }
}

/// The artwork-and-title band a detail page opens with.
fn page_header() -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(28.))
        .mb(px(24.))
}

fn page_title(palette: CadencePalette, title: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(40.))
        .line_height(px(44.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(rgb(palette.text_primary))
        .child(title.into())
}

fn page_detail(palette: CadencePalette, detail: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(14.))
        .text_color(rgb(palette.text_muted))
        .child(detail.into())
}
