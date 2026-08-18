use super::*;

/// How long a library load stays good before an activation may refetch it.
/// Wall clock rather than `Instant`, which stops while the machine sleeps.
const REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Everything Cadence knows about the listener's music: what Spotify holds for
/// the account, and what Cadence keeps locally about it.
///
/// Owned by the services global so a window can be rebuilt without refetching.
pub(super) struct Library {
    backend: BackendHandle,
    liked_tracks: Arc<[model::Track]>,
    playlists: Arc<[model::Playlist]>,
    loaded: bool,
    favorites: Arc<[model::Track]>,
    favorite_keys: HashMap<model::Provider, HashSet<String>>,
    pinned_playlists: Arc<[model::Playlist]>,
    recently_played: Arc<[model::Track]>,
    local_loaded: bool,
    reload: Option<gpui::Task<()>>,
    /// When the contents last arrived, so returning to the window repeatedly
    /// does not refetch the whole library every time.
    refreshed_at: Option<SystemTime>,
}

/// Raised when fresh contents arrived, so stale failures can be cleared.
pub(super) struct LibraryLoaded;

impl EventEmitter<LibraryLoaded> for Library {}

impl Library {
    pub(super) fn new(backend: BackendHandle) -> Self {
        Self {
            backend,
            liked_tracks: Arc::default(),
            playlists: Arc::default(),
            loaded: false,
            favorites: Arc::default(),
            favorite_keys: HashMap::new(),
            pinned_playlists: Arc::default(),
            recently_played: Arc::default(),
            local_loaded: false,
            reload: None,
            refreshed_at: None,
        }
    }

    pub(super) fn reloading(&self) -> bool {
        self.reload.is_some()
    }

    /// Refetches the library, leaving the current contents visible until the
    /// answer arrives. Does nothing when one is already running.
    pub(super) fn revalidate(&mut self, cx: &mut Context<Self>) {
        let fresh = self.refreshed_at.is_some_and(|refreshed_at| {
            refreshed_at
                .elapsed()
                .is_ok_and(|elapsed| elapsed < REFRESH_INTERVAL)
        });
        if self.reload.is_some() || fresh {
            return;
        }
        let (respond, reply) = tokio::sync::oneshot::channel();
        if !self.backend.send(BackendCommand::ReloadLibrary { respond }) {
            return;
        }
        self.reload = Some(cx.spawn(async move |this, cx| {
            let contents = reply.await;
            let _ = this.update(cx, |library, cx| {
                library.reload = None;
                match contents {
                    Ok(Ok((liked_tracks, playlists))) => {
                        library.liked_tracks = liked_tracks.into();
                        library.playlists = playlists.into();
                        library.loaded = true;
                        library.refreshed_at = Some(SystemTime::now());
                        cx.emit(LibraryLoaded);
                    }
                    Ok(Err(error)) => log::warn!("library: refresh failed: {error:#}"),
                    Err(_) => log::warn!("library: backend stopped before answering"),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn liked_tracks(&self) -> &Arc<[model::Track]> {
        &self.liked_tracks
    }

    pub(super) fn playlists(&self) -> &Arc<[model::Playlist]> {
        &self.playlists
    }

    pub(super) fn loaded(&self) -> bool {
        self.loaded
    }

    pub(super) fn favorites(&self) -> &Arc<[model::Track]> {
        &self.favorites
    }

    pub(super) fn pinned_playlists(&self) -> &Arc<[model::Playlist]> {
        &self.pinned_playlists
    }

    pub(super) fn recently_played(&self) -> &Arc<[model::Track]> {
        &self.recently_played
    }

    pub(super) fn local_loaded(&self) -> bool {
        self.local_loaded
    }

    pub(super) fn is_favorite(&self, track: &model::Track) -> bool {
        self.favorite_keys
            .get(&track.provider)
            .is_some_and(|ids| ids.contains(&track.source_id))
    }

    /// Marks the catalog as settled without contents, for when the fetch failed.
    pub(super) fn mark_loaded(&mut self, cx: &mut Context<Self>) {
        self.loaded = true;
        cx.notify();
    }

    pub(super) fn set_favorite(
        &mut self,
        track: model::Track,
        favorite: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        cx.notify();
        self.backend
            .send(BackendCommand::SetFavorite { track, favorite })
    }

    pub(super) fn set_playlist_pinned(
        &mut self,
        playlist: model::Playlist,
        pinned: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        cx.notify();
        self.backend
            .send(BackendCommand::SetPlaylistPinned { playlist, pinned })
    }

    /// Forgets the account's catalog. Locally-owned state stays: it is not tied
    /// to the Spotify account and the backend re-sends it regardless.
    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        self.reload = None;
        self.refreshed_at = None;
        self.liked_tracks = Arc::default();
        self.playlists = Arc::default();
        self.loaded = false;
        cx.notify();
    }

    /// Applies the library half of a backend event, returning the event when the
    /// surrounding app still has its own work to do for it.
    pub(super) fn handle_backend_event(
        &mut self,
        event: BackendEvent,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> Option<BackendEvent> {
        match event {
            BackendEvent::LibraryLoaded {
                generation: loaded_generation,
                liked_tracks,
                playlists,
            } => {
                if loaded_generation == generation {
                    self.liked_tracks = liked_tracks.into();
                    self.playlists = playlists.into();
                    self.loaded = true;
                    self.refreshed_at = Some(SystemTime::now());
                    cx.emit(LibraryLoaded);
                }
            }
            BackendEvent::CachedLikedTracks {
                generation: cached_generation,
                tracks,
            } => {
                if cached_generation == generation {
                    self.liked_tracks = tracks.into();
                }
            }
            BackendEvent::LocalStateLoaded {
                favorites,
                pinned_playlists,
                recently_played,
            } => {
                self.favorite_keys = index_favorites(&favorites);
                self.favorites = favorites.into();
                self.pinned_playlists = pinned_playlists.into();
                self.recently_played = recently_played.into();
                self.local_loaded = true;
                cx.emit(LibraryLoaded);
            }
            event => return Some(event),
        }
        cx.notify();
        None
    }
}
