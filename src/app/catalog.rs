use super::*;

/// What a browsable page reports back to the app around it.
pub(super) enum PageEvent {
    /// Fresh contents arrived, so any stale failure can be cleared.
    Loaded,
    Failed(String),
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
    request_id: u64,
}

impl EventEmitter<PageEvent> for SearchPage {}

impl SearchPage {
    pub(super) fn new(backend: BackendHandle) -> Self {
        Self {
            backend,
            query: String::new(),
            kind: SearchKind::Tracks,
            tracks: Arc::default(),
            playlists: Arc::default(),
            loaded: false,
            searching: false,
            error: None,
            request_id: 0,
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
        let request_id = next_request_id(&mut self.request_id);
        if !self
            .backend
            .send(BackendCommand::SearchCatalog { request_id, query })
        {
            self.loaded = true;
            self.searching = false;
        }
        cx.emit(PageEvent::Loaded);
        cx.notify();
        true
    }

    /// Drops any reply still in flight, for when the account behind it changed.
    pub(super) fn invalidate(&mut self) {
        next_request_id(&mut self.request_id);
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        self.tracks = Arc::default();
        self.playlists = Arc::default();
        self.loaded = false;
        self.searching = false;
        self.error = None;
        cx.notify();
    }

    pub(super) fn handle_backend_event(
        &mut self,
        event: BackendEvent,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> Option<BackendEvent> {
        match event {
            BackendEvent::SearchResults {
                generation: response_generation,
                request_id,
                tracks,
                playlists,
            } => {
                if is_current_response(generation, self.request_id, response_generation, request_id)
                {
                    self.tracks = tracks.into();
                    self.playlists = playlists.into();
                    self.loaded = true;
                    self.searching = false;
                    self.error = None;
                }
            }
            BackendEvent::SearchFailed {
                generation: response_generation,
                request_id,
                error,
            } => {
                if is_current_response(generation, self.request_id, response_generation, request_id)
                {
                    self.loaded = true;
                    self.searching = false;
                    self.error = Some(error.clone());
                    cx.emit(PageEvent::Failed(error));
                }
            }
            event => return Some(event),
        }
        cx.notify();
        None
    }
}

/// The tracks of one Spotify playlist.
pub(super) struct PlaylistPage {
    backend: BackendHandle,
    selected: Option<model::Playlist>,
    tracks: Arc<[model::Track]>,
    loaded: bool,
    error: Option<String>,
    request_id: u64,
}

impl EventEmitter<PageEvent> for PlaylistPage {}

impl PlaylistPage {
    pub(super) fn new(backend: BackendHandle) -> Self {
        Self {
            backend,
            selected: None,
            tracks: Arc::default(),
            loaded: false,
            error: None,
            request_id: 0,
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

    pub(super) fn open(&mut self, playlist: model::Playlist, cx: &mut Context<Self>) {
        self.selected = Some(playlist.clone());
        self.tracks = Arc::default();
        self.loaded = false;
        self.error = None;
        let request_id = next_request_id(&mut self.request_id);
        self.backend.send(BackendCommand::LoadPlaylist {
            request_id,
            playlist,
        });
        cx.notify();
    }

    pub(super) fn invalidate(&mut self) {
        next_request_id(&mut self.request_id);
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        self.tracks = Arc::default();
        self.loaded = false;
        self.error = None;
        cx.notify();
    }

    pub(super) fn handle_backend_event(
        &mut self,
        event: BackendEvent,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> Option<BackendEvent> {
        match event {
            BackendEvent::PlaylistLoaded {
                generation: response_generation,
                request_id,
                playlist,
                tracks,
            } => {
                if is_current_response(generation, self.request_id, response_generation, request_id)
                    && self.selected.as_ref().is_some_and(|selected| {
                        selected.provider == playlist.provider
                            && selected.source_id == playlist.source_id
                    })
                {
                    self.tracks = tracks.into();
                    self.loaded = true;
                    self.error = None;
                    cx.emit(PageEvent::Loaded);
                }
            }
            BackendEvent::PlaylistFailed {
                generation: response_generation,
                request_id,
                source_id,
                error,
            } => {
                if is_current_response(generation, self.request_id, response_generation, request_id)
                    && self
                        .selected
                        .as_ref()
                        .is_some_and(|playlist| playlist.source_id == source_id)
                {
                    self.loaded = true;
                    self.error = Some(error.clone());
                    cx.emit(PageEvent::Failed(error));
                }
            }
            event => return Some(event),
        }
        cx.notify();
        None
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
    loading: bool,
    error: Option<String>,
    loaded_at: Option<Instant>,
    request_id: u64,
}

impl EventEmitter<PageEvent> for ArtistPage {}

impl ArtistPage {
    pub(super) fn new(backend: BackendHandle) -> Self {
        Self {
            backend,
            reference: None,
            artist: None,
            tracks: Arc::default(),
            albums: Arc::default(),
            section: ArtistSection::Popular,
            loaded: false,
            loading: false,
            error: None,
            loaded_at: None,
            request_id: 0,
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

    /// Shows `artist`, refetching unless the cached copy is still fresh.
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
        let should_refresh =
            !same_artist || (!self.loading && !catalog_data_is_fresh(self.loaded_at));
        self.reference = Some(artist);
        self.error = None;
        if !same_artist {
            self.artist = None;
            self.tracks = Arc::default();
            self.albums = Arc::default();
            self.loaded = false;
            self.loading = false;
            self.loaded_at = None;
            self.section = ArtistSection::Popular;
        } else if retrying_failure {
            self.loaded = false;
        }
        if should_refresh {
            let request_id = next_request_id(&mut self.request_id);
            self.loading = true;
            if !self.backend.send(BackendCommand::LoadArtist {
                request_id,
                source_id,
            }) {
                self.loading = false;
                self.loaded = true;
                self.error = Some("Cadence backend is not running".to_owned());
            }
        }
        cx.notify();
        !same_artist
    }

    pub(super) fn invalidate(&mut self) {
        next_request_id(&mut self.request_id);
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        self.reference = None;
        self.artist = None;
        self.tracks = Arc::default();
        self.albums = Arc::default();
        self.loaded = false;
        self.loading = false;
        self.error = None;
        self.loaded_at = None;
        cx.notify();
    }

    pub(super) fn handle_backend_event(
        &mut self,
        event: BackendEvent,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> Option<BackendEvent> {
        match event {
            BackendEvent::ArtistLoaded {
                generation: response_generation,
                request_id,
                source_id,
                artist,
                tracks,
                albums,
            } => {
                if is_current_response(generation, self.request_id, response_generation, request_id)
                    && self.matches(&source_id)
                {
                    self.artist = Some(artist);
                    self.tracks = tracks.into();
                    self.albums = albums.into();
                    self.loaded = true;
                    self.loading = false;
                    self.error = None;
                    self.loaded_at = Some(Instant::now());
                    cx.emit(PageEvent::Loaded);
                }
            }
            BackendEvent::ArtistFailed {
                generation: response_generation,
                request_id,
                source_id,
                error,
            } => {
                if is_current_response(generation, self.request_id, response_generation, request_id)
                    && self.matches(&source_id)
                {
                    self.loading = false;
                    if !self.loaded {
                        self.loaded = true;
                        self.error = Some(error.clone());
                    }
                    cx.emit(PageEvent::Failed(error));
                }
            }
            event => return Some(event),
        }
        cx.notify();
        None
    }

    fn matches(&self, source_id: &str) -> bool {
        self.reference
            .as_ref()
            .and_then(|artist| artist.source_id.as_deref())
            == Some(source_id)
    }
}

/// One album and its tracks.
pub(super) struct AlbumPage {
    backend: BackendHandle,
    reference: Option<model::AlbumRef>,
    album: Option<model::Album>,
    tracks: Arc<[model::Track]>,
    loaded: bool,
    loading: bool,
    error: Option<String>,
    loaded_at: Option<Instant>,
    request_id: u64,
}

impl EventEmitter<PageEvent> for AlbumPage {}

impl AlbumPage {
    pub(super) fn new(backend: BackendHandle) -> Self {
        Self {
            backend,
            reference: None,
            album: None,
            tracks: Arc::default(),
            loaded: false,
            loading: false,
            error: None,
            loaded_at: None,
            request_id: 0,
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

    /// Shows `album`, refetching unless the cached copy is still fresh.
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
        let should_refresh =
            !same_album || (!self.loading && !catalog_data_is_fresh(self.loaded_at));
        self.reference = Some(album);
        self.error = None;
        if !same_album {
            self.album = None;
            self.tracks = Arc::default();
            self.loaded = false;
            self.loading = false;
            self.loaded_at = None;
        } else if retrying_failure {
            self.loaded = false;
        }
        if should_refresh {
            let request_id = next_request_id(&mut self.request_id);
            self.loading = true;
            if !self.backend.send(BackendCommand::LoadAlbum {
                request_id,
                source_id,
            }) {
                self.loading = false;
                self.loaded = true;
                self.error = Some("Cadence backend is not running".to_owned());
            }
        }
        cx.notify();
        !same_album
    }

    pub(super) fn invalidate(&mut self) {
        next_request_id(&mut self.request_id);
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        self.reference = None;
        self.album = None;
        self.tracks = Arc::default();
        self.loaded = false;
        self.loading = false;
        self.error = None;
        self.loaded_at = None;
        cx.notify();
    }

    pub(super) fn handle_backend_event(
        &mut self,
        event: BackendEvent,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> Option<BackendEvent> {
        match event {
            BackendEvent::AlbumLoaded {
                generation: response_generation,
                request_id,
                source_id,
                album,
                tracks,
            } => {
                if is_current_response(generation, self.request_id, response_generation, request_id)
                    && self.matches(&source_id)
                {
                    self.album = Some(album);
                    self.tracks = tracks.into();
                    self.loaded = true;
                    self.loading = false;
                    self.error = None;
                    self.loaded_at = Some(Instant::now());
                    cx.emit(PageEvent::Loaded);
                }
            }
            BackendEvent::AlbumFailed {
                generation: response_generation,
                request_id,
                source_id,
                error,
            } => {
                if is_current_response(generation, self.request_id, response_generation, request_id)
                    && self.matches(&source_id)
                {
                    self.loading = false;
                    if !self.loaded {
                        self.loaded = true;
                        self.error = Some(error.clone());
                    }
                    cx.emit(PageEvent::Failed(error));
                }
            }
            event => return Some(event),
        }
        cx.notify();
        None
    }

    fn matches(&self, source_id: &str) -> bool {
        self.reference
            .as_ref()
            .and_then(|album| album.source_id.as_deref())
            == Some(source_id)
    }
}
