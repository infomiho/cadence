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
    image_cache: Entity<image_cache::BoundedImageCache>,
    lifecycle: Arc<InstanceLifecycle>,
    preferences: Option<Store>,
}

impl gpui::Global for AppServices {}

impl AppServices {
    pub(super) fn init(
        cx: &mut App,
        lifecycle: Arc<InstanceLifecycle>,
        preferences: Option<Store>,
    ) -> (BackendHandle, BackendEvents) {
        let (backend, events) = Backend::start();
        let handle = backend.handle();
        let player = cx.new(|_| player::Player::new(handle.clone()));
        let session = cx.new(|_| session::Session::new(handle.clone()));
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
            image_cache,
            lifecycle,
            preferences,
        });
        (handle, events)
    }

    /// Playback outlives windows, so the player is owned here and shared by handle.
    pub(super) fn player(cx: &App) -> Entity<player::Player> {
        cx.global::<Self>().player.clone()
    }

    /// The signed-in account, which outlives any window showing it.
    pub(super) fn session(cx: &App) -> Entity<session::Session> {
        cx.global::<Self>().session.clone()
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

    /// Restarts the backend after a fatal failure, returning the replacements
    /// for the handle and event stream the previous backend owned.
    pub(super) fn restart(cx: &mut App) -> (BackendHandle, BackendEvents) {
        let (backend, events) = Backend::start();
        let handle = backend.handle();
        let (player, session) = {
            let services = cx.global_mut::<Self>();
            services.backend = Some(backend);
            (services.player.clone(), services.session.clone())
        };
        player.update(cx, |player, _| player.connect(handle.clone()));
        session.update(cx, |session, cx| session.connect(handle.clone(), cx));
        (handle, events)
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
