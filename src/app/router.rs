use super::*;

/// Where the workspace is, and where each detail route came from.
///
/// Detail routes are reachable from several places, so each remembers the
/// route that opened it. That is what the back button and the sidebar's pinned
/// playlists navigate to.
#[derive(Clone, Copy)]
pub(super) struct Router {
    route: Route,
    playlist_origin: Route,
    artist_origin: Route,
    album_origin: Route,
    settings_origin: Route,
}

impl Router {
    pub(super) fn new() -> Self {
        Self {
            route: Route::LikedSongs,
            playlist_origin: Route::Playlists,
            artist_origin: Route::LikedSongs,
            album_origin: Route::LikedSongs,
            settings_origin: Route::LikedSongs,
        }
    }

    pub(super) fn route(self) -> Route {
        self.route
    }

    pub(super) fn navigate(&mut self, route: Route) {
        self.route = route;
    }

    pub(super) fn open_playlist(&mut self, origin: Route) {
        self.playlist_origin = origin;
        self.route = Route::Playlist;
    }

    /// Opens the artist page. `changed` says the page is showing a different
    /// artist, which is what makes revisiting the same one from the same place
    /// keep the original way back rather than pointing at itself.
    pub(super) fn open_artist(&mut self, origin: Route, changed: bool) {
        if changed || self.route != Route::Artist {
            self.artist_origin = origin;
        }
        self.route = Route::Artist;
    }

    pub(super) fn open_album(&mut self, origin: Route, changed: bool) {
        if changed || self.route != Route::Album {
            self.album_origin = origin;
        }
        self.route = Route::Album;
    }

    pub(super) fn open_settings(&mut self) {
        self.settings_origin = self.back_target().unwrap_or(self.route);
        self.route = Route::Settings;
    }

    /// Where the back button goes, for the routes that have one.
    pub(super) fn back_target(self) -> Option<Route> {
        match self.route {
            Route::Playlist => Some(self.playlist_origin),
            Route::Artist => Some(self.artist_origin),
            Route::Album => Some(self.album_origin),
            Route::Settings => Some(self.settings_origin),
            _ => None,
        }
    }

    /// Which route a pinned playlist should return to once the listener backs
    /// out of it. Opening one from a playlist keeps that playlist's own origin
    /// so the trail does not loop back on itself.
    pub(super) fn pinned_origin(self) -> Route {
        if self.route == Route::Playlist {
            self.playlist_origin
        } else {
            self.route
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Route, Router};

    #[test]
    fn detail_routes_go_back_to_wherever_they_were_opened_from() {
        let mut router = Router::new();

        router.navigate(Route::Search);
        router.open_playlist(Route::Search);

        assert_eq!(router.route(), Route::Playlist);
        assert_eq!(router.back_target(), Some(Route::Search));
    }

    #[test]
    fn routes_without_a_detail_view_have_nowhere_to_go_back_to() {
        let mut router = Router::new();

        router.navigate(Route::LikedSongs);

        assert_eq!(router.back_target(), None);
    }

    #[test]
    fn reopening_the_same_artist_keeps_the_original_way_back() {
        let mut router = Router::new();

        router.open_artist(Route::LikedSongs, true);
        router.open_artist(Route::Artist, false);

        assert_eq!(router.back_target(), Some(Route::LikedSongs));
    }

    #[test]
    fn opening_a_different_artist_from_the_artist_page_moves_the_way_back() {
        let mut router = Router::new();

        router.open_artist(Route::LikedSongs, true);
        router.open_artist(Route::Artist, true);

        assert_eq!(router.back_target(), Some(Route::Artist));
    }

    #[test]
    fn settings_returns_past_a_detail_route_to_where_that_route_came_from() {
        let mut router = Router::new();

        router.navigate(Route::Recent);
        router.open_album(Route::Recent, true);
        router.open_settings();

        assert_eq!(router.back_target(), Some(Route::Recent));
    }

    #[test]
    fn reopening_settings_does_not_make_settings_its_own_way_back() {
        let mut router = Router::new();

        router.navigate(Route::Favorites);
        router.open_settings();
        router.open_settings();

        assert_eq!(router.back_target(), Some(Route::Favorites));
    }

    #[test]
    fn a_pinned_playlist_opened_from_a_playlist_returns_past_it() {
        let mut router = Router::new();

        router.navigate(Route::Recent);
        router.open_playlist(Route::Recent);

        assert_eq!(router.pinned_origin(), Route::Recent);
    }
}
