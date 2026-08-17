use super::*;

/// Services that outlive any single window.
///
/// Closing the Cadence window must not interrupt playback, so the backend is
/// owned here instead of by the view that happens to be on screen.
pub(super) struct AppServices {
    /// Taken during shutdown so the worker thread stops before the process exits.
    backend: Option<Backend>,
    player: Entity<player::Player>,
    session: Entity<session::Session>,
    library: Entity<library::Library>,
    image_cache: Entity<image_cache::BoundedImageCache>,
    /// The window currently showing these services, if one is open.
    root: Option<gpui::WeakEntity<CadenceApp>>,
    /// Drains backend events for the whole process, not just for a window.
    event_pump: Option<gpui::Task<()>>,
    lifecycle: Arc<InstanceLifecycle>,
    preferences: Option<Store>,
}

impl gpui::Global for AppServices {}

impl AppServices {
    pub(super) fn init(
        cx: &mut App,
        lifecycle: Arc<InstanceLifecycle>,
        preferences: Option<Store>,
    ) -> BackendHandle {
        let (backend, events) = Backend::start();
        let handle = backend.handle();
        let player = cx.new(|_| player::Player::new(handle.clone()));
        let session = cx.new(|_| session::Session::new(handle.clone()));
        let library = cx.new(|_| library::Library::new(handle.clone()));
        let image_cache = image_cache::BoundedImageCache::new(cx);
        cx.on_app_quit(|cx| {
            Self::shutdown(cx);
            async {}
        })
        .detach();
        // Until the window can be reopened, the app is only reachable while a
        // window exists, so the last one closing ends the session.
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.set_global(Self {
            backend: Some(backend),
            player,
            session,
            library,
            image_cache,
            root: None,
            event_pump: None,
            lifecycle,
            preferences,
        });
        Self::pump(events, cx);
        handle
    }

    /// Playback outlives windows, so the player is owned here and shared by handle.
    pub(super) fn player(cx: &App) -> Entity<player::Player> {
        cx.global::<Self>().player.clone()
    }

    /// The signed-in account, which outlives any window showing it.
    pub(super) fn session(cx: &App) -> Entity<session::Session> {
        cx.global::<Self>().session.clone()
    }

    /// The listener's music, which outlives any window showing it.
    pub(super) fn library(cx: &App) -> Entity<library::Library> {
        cx.global::<Self>().library.clone()
    }

    /// Artwork is shared by every view, so the cache is not tied to one of them.
    pub(super) fn image_cache(cx: &App) -> Entity<image_cache::BoundedImageCache> {
        cx.global::<Self>().image_cache.clone()
    }

    /// Notifications that another launch of Cadence asked this instance to come
    /// to the front.
    pub(super) fn activations(cx: &App) -> async_channel::Receiver<()> {
        cx.global::<Self>().lifecycle.activation_receiver()
    }

    /// Runs `save` against the preferences store, if one could be opened.
    pub(super) fn with_preferences<R>(
        cx: &mut App,
        save: impl FnOnce(&mut Store) -> R,
    ) -> Option<R> {
        cx.global_mut::<Self>().preferences.as_mut().map(save)
    }

    /// Notes which window should receive the events the services do not consume.
    pub(super) fn set_root(root: gpui::WeakEntity<CadenceApp>, cx: &mut App) {
        cx.global_mut::<Self>().root = Some(root);
    }

    /// Drains backend events for the process, so playback keeps advancing even
    /// when no window is open to watch it.
    fn pump(mut events: BackendEvents, cx: &mut App) {
        let task = cx.spawn(async move |cx| {
            while let Some(batch) = receive_backend_event_batch(&mut events).await {
                if cx.update(|cx| Self::dispatch(batch, cx)).is_err() {
                    break;
                }
            }
        });
        cx.global_mut::<Self>().event_pump = Some(task);
    }

    fn dispatch(events: Vec<BackendEvent>, cx: &mut App) {
        let (player, session, library, root) = {
            let services = cx.global::<Self>();
            (
                services.player.clone(),
                services.session.clone(),
                services.library.clone(),
                services.root.clone(),
            )
        };
        let mut unhandled = Vec::new();
        for event in events {
            let Some(event) =
                player.update(cx, |player, cx| player.handle_backend_event(event, cx))
            else {
                continue;
            };
            let Some(event) =
                session.update(cx, |session, cx| session.handle_backend_event(event, cx))
            else {
                continue;
            };
            let generation = session.read(cx).generation();
            let Some(event) = library.update(cx, |library, cx| {
                library.handle_backend_event(event, generation, cx)
            }) else {
                continue;
            };
            unhandled.push(event);
        }
        if unhandled.is_empty() {
            return;
        }
        if let Some(root) = root.and_then(|root| root.upgrade()) {
            root.update(cx, |root, cx| root.handle_backend_events(unhandled, cx));
        }
    }

    /// Restarts the backend after a fatal failure.
    pub(super) fn restart(cx: &mut App) -> BackendHandle {
        let (backend, events) = Backend::start();
        let handle = backend.handle();
        let (player, session, library) = {
            let services = cx.global_mut::<Self>();
            services.backend = Some(backend);
            (
                services.player.clone(),
                services.session.clone(),
                services.library.clone(),
            )
        };
        player.update(cx, |player, _| player.connect(handle.clone()));
        session.update(cx, |session, cx| session.connect(handle.clone(), cx));
        library.update(cx, |library, _| library.connect(handle.clone()));
        Self::pump(events, cx);
        handle
    }

    /// Saves the live position and stops the worker thread. The process exits
    /// straight after `applicationWillTerminate:`, so nothing else will.
    fn shutdown(cx: &mut App) {
        let player = cx.global::<Self>().player.clone();
        player.read(cx).save_position();
        let backend = cx.global_mut::<Self>().backend.take();
        drop(backend);
    }
}
