use super::*;

use page::PageEvent;
use track_list::{PlaylistList, TrackList};

/// Sends `command` and hands back the reply channel to await.
///
/// Dropping the returned future is what cancels the request: the page holds it
/// in a `Task`, so starting a new request drops the previous one and its answer
/// is discarded rather than overwriting fresher state.
fn request<T, C>(
    backend: &BackendHandle,
    command: C,
) -> impl Future<Output = Result<T, String>> + use<T, C>
where
    T: Send + 'static,
    C: FnOnce(Reply<T>) -> BackendCommand,
{
    let (respond, reply) = tokio::sync::oneshot::channel();
    let sent = backend.send(command(respond));
    async move {
        if !sent {
            return Err("Cadence backend is busy or not running".to_owned());
        }
        match reply.await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => Err(format!("{error:#}")),
            Err(_) => Err("Cadence backend stopped before answering".to_owned()),
        }
    }
}

/// Search results for the current query.
pub(super) struct SearchPage {
    backend: BackendHandle,
    query: String,
    kind: SearchKind,
    tracks: Arc<[model::Track]>,
    playlists: Arc<[model::Playlist]>,
    loaded: bool,
    searching: bool,
    error: Option<String>,
    request: Option<gpui::Task<()>>,
    pub(super) track_list: Entity<TrackList>,
    pub(super) playlist_list: Entity<PlaylistList>,
    _list_subscriptions: [Subscription; 2],
}

impl EventEmitter<PageEvent> for SearchPage {}

impl SearchPage {
    pub(super) fn new(backend: BackendHandle, cx: &mut Context<Self>) -> Self {
        let track_list = cx.new(|cx| TrackList::new("search-tracks", cx));
        let playlist_list = cx.new(|cx| PlaylistList::new("search-playlists", cx));
        Self {
            backend,
            query: String::new(),
            kind: SearchKind::Tracks,
            tracks: Arc::default(),
            playlists: Arc::default(),
            loaded: false,
            searching: false,
            error: None,
            request: None,
            _list_subscriptions: [
                page::forward(&track_list, cx),
                page::forward(&playlist_list, cx),
            ],
            track_list,
            playlist_list,
        }
    }

    pub(super) fn query(&self) -> &str {
        &self.query
    }

    pub(super) fn set_query(&mut self, query: String, cx: &mut Context<Self>) {
        self.query = query;
        cx.notify();
    }

    pub(super) fn kind(&self) -> SearchKind {
        self.kind
    }

    pub(super) fn set_kind(&mut self, kind: SearchKind, cx: &mut Context<Self>) {
        self.kind = kind;
        cx.notify();
    }

    pub(super) fn tracks(&self) -> &Arc<[model::Track]> {
        &self.tracks
    }

    pub(super) fn playlists(&self) -> &Arc<[model::Playlist]> {
        &self.playlists
    }

    pub(super) fn loaded(&self) -> bool {
        self.loaded
    }

    pub(super) fn searching(&self) -> bool {
        self.searching
    }

    pub(super) fn error(&self) -> Option<&String> {
        self.error.as_ref()
    }

    /// Runs the trimmed query, reporting whether there was one to run.
    pub(super) fn submit(&mut self, cx: &mut Context<Self>) -> bool {
        let query = self.query.trim().to_owned();
        if query.is_empty() {
            return false;
        }
        self.loaded = false;
        self.searching = true;
        self.error = None;
        let reply = request(&self.backend, |respond| BackendCommand::SearchCatalog {
            query,
            respond,
        });
        self.request = Some(cx.spawn(async move |this, cx| {
            let result = reply.await;
            let _ = this.update(cx, |page, cx| {
                page.request = None;
                page.searching = false;
                page.loaded = true;
                match result {
                    Ok((tracks, playlists)) => {
                        page.tracks = tracks.into();
                        page.playlists = playlists.into();
                        page.error = None;
                        cx.emit(PageEvent::Loaded);
                    }
                    Err(error) => {
                        page.error = Some(error.clone());
                        cx.emit(PageEvent::Failed(error));
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
        true
    }

    /// Takes down any open row menu, for a route change no click drove.
    pub(super) fn close_menus(&mut self, cx: &mut Context<Self>) {
        self.track_list.update(cx, |list, cx| list.close_menu(cx));
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        self.request = None;
        self.tracks = Arc::default();
        self.playlists = Arc::default();
        self.loaded = false;
        self.searching = false;
        self.error = None;
        cx.notify();
    }
}

/// The tracks of one Spotify playlist.
pub(super) struct PlaylistPage {
    backend: BackendHandle,
    selected: Option<model::Playlist>,
    tracks: Arc<[model::Track]>,
    loaded: bool,
    error: Option<String>,
    request: Option<gpui::Task<()>>,
    pub(super) library: Entity<library::Library>,
    player: Entity<player::Player>,
    pub(super) image_cache: Entity<image_cache::BoundedImageCache>,
    pub(super) track_list: Entity<TrackList>,
    _list_subscription: Subscription,
}

impl EventEmitter<PageEvent> for PlaylistPage {}

impl PlaylistPage {
    pub(super) fn new(backend: BackendHandle, cx: &mut Context<Self>) -> Self {
        let track_list = cx.new(|cx| TrackList::new("playlist-tracks", cx));
        Self {
            backend,
            selected: None,
            tracks: Arc::default(),
            loaded: false,
            error: None,
            request: None,
            library: services::AppServices::library(cx),
            player: services::AppServices::player(cx),
            image_cache: services::AppServices::image_cache(cx),
            _list_subscription: page::forward(&track_list, cx),
            track_list,
        }
    }

    pub(super) fn selected(&self) -> Option<&model::Playlist> {
        self.selected.as_ref()
    }

    pub(super) fn tracks(&self) -> &Arc<[model::Track]> {
        &self.tracks
    }

    pub(super) fn loaded(&self) -> bool {
        self.loaded
    }

    pub(super) fn error(&self) -> Option<&String> {
        self.error.as_ref()
    }

    /// Takes down any open row menu, for a route change no click drove.
    pub(super) fn close_menus(&mut self, cx: &mut Context<Self>) {
        self.track_list.update(cx, |list, cx| list.close_menu(cx));
    }

    /// Starts the page's contents from the top, if there is anything to play.
    pub(super) fn play(&mut self, tracks: &Arc<[model::Track]>, cx: &mut Context<Self>) {
        if tracks.is_empty() {
            return;
        }
        let tracks = tracks.to_vec();
        self.player
            .update(cx, |player, cx| player.play_context(tracks, 0, cx));
    }

    pub(super) fn open(&mut self, playlist: model::Playlist, cx: &mut Context<Self>) {
        self.selected = Some(playlist.clone());
        self.tracks = Arc::default();
        self.loaded = false;
        self.error = None;
        let reply = request(&self.backend, |respond| BackendCommand::LoadPlaylist {
            playlist,
            respond,
        });
        self.request = Some(cx.spawn(async move |this, cx| {
            let result = reply.await;
            let _ = this.update(cx, |page, cx| {
                page.request = None;
                page.loaded = true;
                match result {
                    Ok(tracks) => {
                        page.tracks = tracks.into();
                        page.error = None;
                        cx.emit(PageEvent::Loaded);
                    }
                    Err(error) => {
                        page.error = Some(error.clone());
                        cx.emit(PageEvent::Failed(error));
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        self.request = None;
        self.selected = None;
        self.tracks = Arc::default();
        self.loaded = false;
        self.error = None;
        cx.notify();
    }
}

/// One artist, their popular tracks and their discography.
pub(super) struct ArtistPage {
    backend: BackendHandle,
    reference: Option<model::ArtistRef>,
    artist: Option<model::Artist>,
    tracks: Arc<[model::Track]>,
    albums: Arc<[model::Album]>,
    section: ArtistSection,
    loaded: bool,
    error: Option<String>,
    loaded_at: Option<SystemTime>,
    request: Option<gpui::Task<()>>,
    pub(super) image_cache: Entity<image_cache::BoundedImageCache>,
    pub(super) track_list: Entity<TrackList>,
    _list_subscription: Subscription,
}

impl EventEmitter<PageEvent> for ArtistPage {}

impl ArtistPage {
    pub(super) fn new(backend: BackendHandle, cx: &mut Context<Self>) -> Self {
        let track_list = cx.new(|cx| TrackList::new("artist-popular", cx));
        Self {
            backend,
            reference: None,
            artist: None,
            tracks: Arc::default(),
            albums: Arc::default(),
            section: ArtistSection::Popular,
            loaded: false,
            error: None,
            loaded_at: None,
            request: None,
            image_cache: services::AppServices::image_cache(cx),
            _list_subscription: page::forward(&track_list, cx),
            track_list,
        }
    }

    pub(super) fn reference(&self) -> Option<&model::ArtistRef> {
        self.reference.as_ref()
    }

    pub(super) fn artist(&self) -> Option<&model::Artist> {
        self.artist.as_ref()
    }

    pub(super) fn tracks(&self) -> &Arc<[model::Track]> {
        &self.tracks
    }

    pub(super) fn albums(&self) -> &Arc<[model::Album]> {
        &self.albums
    }

    pub(super) fn section(&self) -> ArtistSection {
        self.section
    }

    pub(super) fn set_section(&mut self, section: ArtistSection, cx: &mut Context<Self>) {
        self.section = section;
        cx.notify();
    }

    pub(super) fn loaded(&self) -> bool {
        self.loaded
    }

    pub(super) fn error(&self) -> Option<&String> {
        self.error.as_ref()
    }

    /// Takes down any open row menu, for a route change no click drove.
    pub(super) fn close_menus(&mut self, cx: &mut Context<Self>) {
        self.track_list.update(cx, |list, cx| list.close_menu(cx));
    }

    /// Shows `artist`, refetching unless the cached copy is still fresh.
    /// Reports whether this is a different artist than the one already shown.
    pub(super) fn open(&mut self, artist: model::ArtistRef, cx: &mut Context<Self>) -> bool {
        let Some(source_id) = artist.source_id.clone() else {
            return false;
        };
        let same_artist = self
            .reference
            .as_ref()
            .and_then(|artist| artist.source_id.as_deref())
            == Some(source_id.as_str());
        let retrying_failure = same_artist && self.error.is_some();
        let loading = self.request.is_some();
        let should_refresh = !same_artist || (!loading && !catalog_data_is_fresh(self.loaded_at));
        self.reference = Some(artist);
        self.error = None;
        if !same_artist {
            self.artist = None;
            self.tracks = Arc::default();
            self.albums = Arc::default();
            self.loaded = false;
            self.loaded_at = None;
            self.section = ArtistSection::Popular;
        } else if retrying_failure {
            self.loaded = false;
        }
        if should_refresh {
            let reply = request(&self.backend, |respond| BackendCommand::LoadArtist {
                source_id,
                respond,
            });
            self.request = Some(cx.spawn(async move |this, cx| {
                let result = reply.await;
                let _ = this.update(cx, |page, cx| {
                    page.request = None;
                    match result {
                        Ok((artist, tracks, albums)) => {
                            page.artist = Some(artist);
                            page.tracks = tracks.into();
                            page.albums = albums.into();
                            page.loaded = true;
                            page.error = None;
                            page.loaded_at = Some(SystemTime::now());
                            cx.emit(PageEvent::Loaded);
                        }
                        Err(error) => {
                            if !page.loaded {
                                page.loaded = true;
                                page.error = Some(error.clone());
                            }
                            cx.emit(PageEvent::Failed(error));
                        }
                    }
                    cx.notify();
                });
            }));
        }
        cx.notify();
        !same_artist
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        self.request = None;
        self.reference = None;
        self.artist = None;
        self.tracks = Arc::default();
        self.albums = Arc::default();
        self.loaded = false;
        self.error = None;
        self.loaded_at = None;
        cx.notify();
    }
}

/// One album and its tracks.
pub(super) struct AlbumPage {
    backend: BackendHandle,
    reference: Option<model::AlbumRef>,
    album: Option<model::Album>,
    tracks: Arc<[model::Track]>,
    loaded: bool,
    error: Option<String>,
    loaded_at: Option<SystemTime>,
    request: Option<gpui::Task<()>>,
    player: Entity<player::Player>,
    pub(super) image_cache: Entity<image_cache::BoundedImageCache>,
    pub(super) track_list: Entity<TrackList>,
    _list_subscription: Subscription,
}

impl EventEmitter<PageEvent> for AlbumPage {}

impl AlbumPage {
    pub(super) fn new(backend: BackendHandle, cx: &mut Context<Self>) -> Self {
        let track_list = cx.new(|cx| TrackList::new("album-tracks", cx));
        Self {
            backend,
            reference: None,
            album: None,
            tracks: Arc::default(),
            loaded: false,
            error: None,
            loaded_at: None,
            request: None,
            player: services::AppServices::player(cx),
            image_cache: services::AppServices::image_cache(cx),
            _list_subscription: page::forward(&track_list, cx),
            track_list,
        }
    }

    pub(super) fn reference(&self) -> Option<&model::AlbumRef> {
        self.reference.as_ref()
    }

    pub(super) fn album(&self) -> Option<&model::Album> {
        self.album.as_ref()
    }

    pub(super) fn tracks(&self) -> &Arc<[model::Track]> {
        &self.tracks
    }

    pub(super) fn loaded(&self) -> bool {
        self.loaded
    }

    pub(super) fn error(&self) -> Option<&String> {
        self.error.as_ref()
    }

    /// Takes down any open row menu, for a route change no click drove.
    pub(super) fn close_menus(&mut self, cx: &mut Context<Self>) {
        self.track_list.update(cx, |list, cx| list.close_menu(cx));
    }

    /// Starts the page's contents from the top, if there is anything to play.
    pub(super) fn play(&mut self, tracks: &Arc<[model::Track]>, cx: &mut Context<Self>) {
        if tracks.is_empty() {
            return;
        }
        let tracks = tracks.to_vec();
        self.player
            .update(cx, |player, cx| player.play_context(tracks, 0, cx));
    }

    /// Shows `album`, refetching unless the cached copy is still fresh.
    /// Reports whether this is a different album than the one already shown.
    pub(super) fn open(&mut self, album: model::AlbumRef, cx: &mut Context<Self>) -> bool {
        let Some(source_id) = album.source_id.clone() else {
            return false;
        };
        let same_album = self
            .reference
            .as_ref()
            .and_then(|album| album.source_id.as_deref())
            == Some(source_id.as_str());
        let retrying_failure = same_album && self.error.is_some();
        let loading = self.request.is_some();
        let should_refresh = !same_album || (!loading && !catalog_data_is_fresh(self.loaded_at));
        self.reference = Some(album);
        self.error = None;
        if !same_album {
            self.album = None;
            self.tracks = Arc::default();
            self.loaded = false;
            self.loaded_at = None;
        } else if retrying_failure {
            self.loaded = false;
        }
        if should_refresh {
            let reply = request(&self.backend, |respond| BackendCommand::LoadAlbum {
                source_id,
                respond,
            });
            self.request = Some(cx.spawn(async move |this, cx| {
                let result = reply.await;
                let _ = this.update(cx, |page, cx| {
                    page.request = None;
                    match result {
                        Ok((album, tracks)) => {
                            page.album = Some(album);
                            page.tracks = tracks.into();
                            page.loaded = true;
                            page.error = None;
                            page.loaded_at = Some(SystemTime::now());
                            cx.emit(PageEvent::Loaded);
                        }
                        Err(error) => {
                            if !page.loaded {
                                page.loaded = true;
                                page.error = Some(error.clone());
                            }
                            cx.emit(PageEvent::Failed(error));
                        }
                    }
                    cx.notify();
                });
            }));
        }
        cx.notify();
        !same_album
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        self.request = None;
        self.reference = None;
        self.album = None;
        self.tracks = Arc::default();
        self.loaded = false;
        self.error = None;
        self.loaded_at = None;
        cx.notify();
    }
}
