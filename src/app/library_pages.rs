use super::*;

use page::PageEvent;

/// One of the track collections the library keeps for the signed-in account.
///
/// The three read different slices of `Library` and word themselves
/// differently, but the page around them is the same, so they share one entity.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LibrarySection {
    LikedSongs,
    Favorites,
    Recent,
}

impl LibrarySection {
    fn page_id(self) -> &'static str {
        match self {
            Self::LikedSongs => "liked-songs-page",
            Self::Favorites => "favorites-page",
            Self::Recent => "recent-page",
        }
    }

    fn list_id(self) -> &'static str {
        match self {
            Self::LikedSongs => "liked-tracks",
            Self::Favorites => "favorite-tracks",
            Self::Recent => "recent-tracks",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::LikedSongs => "Liked Songs",
            Self::Favorites => "Favorites",
            Self::Recent => "Recently played",
        }
    }

    fn empty_message(self) -> &'static str {
        match self {
            Self::LikedSongs => "No liked songs",
            Self::Favorites => "No favorites yet",
            Self::Recent => "No listening history yet",
        }
    }

    fn loading_message(self) -> &'static str {
        match self {
            Self::LikedSongs => "Loading liked songs…",
            Self::Favorites => "Loading favorites…",
            Self::Recent => "Loading listening history…",
        }
    }

    fn tracks(self, library: &library::Library) -> Arc<[model::Track]> {
        match self {
            Self::LikedSongs => library.liked_tracks().clone(),
            Self::Favorites => library.favorites().clone(),
            Self::Recent => library.recently_played().clone(),
        }
    }

    /// Whether an empty collection means "nothing here" rather than "not yet".
    /// Liked songs come from Spotify; the other two are local state.
    fn loaded(self, library: &library::Library) -> bool {
        match self {
            Self::LikedSongs => library.loaded(),
            Self::Favorites | Self::Recent => library.local_loaded(),
        }
    }

    fn detail(self, library: &library::Library, track_count: usize) -> String {
        match self {
            Self::LikedSongs => components::revalidating_detail(
                if library.loaded() {
                    format!("{track_count} tracks loaded from Spotify")
                } else {
                    "Liked on Spotify".to_owned()
                },
                library.reloading(),
            ),
            Self::Favorites => "Starred in Cadence".to_owned(),
            Self::Recent => "Listening history".to_owned(),
        }
    }
}

/// A saved collection of tracks, listed straight from the library.
pub(super) struct LibraryTracksPage {
    section: LibrarySection,
    library: Entity<library::Library>,
    tracks: Entity<track_list::TrackList>,
    _tracks_subscription: Subscription,
}

impl EventEmitter<PageEvent> for LibraryTracksPage {}

impl LibraryTracksPage {
    pub(super) fn new(section: LibrarySection, cx: &mut Context<Self>) -> Self {
        let tracks = cx.new(|cx| track_list::TrackList::new(cx));
        Self {
            section,
            library: services::AppServices::library(cx),
            _tracks_subscription: page::forward(&tracks, cx),
            tracks,
        }
    }

    /// Takes down any open row menu, for a route change no click drove.
    pub(super) fn close_menus(&mut self, cx: &mut Context<Self>) {
        self.tracks.update(cx, |list, cx| list.close_menu(cx));
    }
}

impl Render for LibraryTracksPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::Appearance::palette(cx);
        let section = self.section;
        let (tracks, loaded, detail) = {
            let library = self.library.read(cx);
            let tracks = section.tracks(library);
            (
                tracks.clone(),
                section.loaded(library),
                section.detail(library, tracks.len()),
            )
        };
        let content = if tracks.is_empty() {
            let message = if loaded {
                section.empty_message()
            } else {
                section.loading_message()
            };
            components::empty_state(palette, message).into_any_element()
        } else {
            self.tracks
                .update(cx, |list, cx| list.show(section.list_id(), tracks, cx));
            self.tracks.clone().into_any_element()
        };

        components::page(section.page_id())
            .pt(px(12.))
            .child(components::page_heading(palette, section.title(), detail))
            .child(content)
    }
}

/// Every playlist the account follows on Spotify.
pub(super) struct PlaylistsPage {
    library: Entity<library::Library>,
    playlists: Entity<track_list::PlaylistList>,
    _playlists_subscription: Subscription,
}

impl EventEmitter<PageEvent> for PlaylistsPage {}

impl PlaylistsPage {
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        let playlists = cx.new(|cx| track_list::PlaylistList::new(cx));
        Self {
            library: services::AppServices::library(cx),
            _playlists_subscription: page::forward(&playlists, cx),
            playlists,
        }
    }
}

impl Render for PlaylistsPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::Appearance::palette(cx);
        let (playlists, loaded, detail) = {
            let library = self.library.read(cx);
            (
                library.playlists().clone(),
                library.loaded(),
                components::revalidating_detail("Your Spotify playlists", library.reloading()),
            )
        };
        let content = if playlists.is_empty() {
            let message = if loaded {
                "No Spotify playlists"
            } else {
                "Loading playlists…"
            };
            components::empty_state(palette, message).into_any_element()
        } else {
            self.playlists
                .update(cx, |list, cx| list.show("spotify-playlists", playlists, cx));
            self.playlists.clone().into_any_element()
        };

        components::page("playlists-page")
            .pt(px(12.))
            .child(components::page_heading(palette, "Playlists", detail))
            .child(content)
    }
}
