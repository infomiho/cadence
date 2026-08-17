use super::*;

/// Services that outlive any single window.
///
/// Closing the Cadence window must not interrupt playback, so the backend is
/// owned here instead of by the view that happens to be on screen.
pub(super) struct AppServices {
    /// Taken during shutdown so the worker thread stops before the process exits.
    backend: Option<Backend>,
    player: Entity<player::Player>,
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
            lifecycle,
            preferences,
        });
        (handle, events)
    }

    /// Playback outlives windows, so the player is owned here and shared by handle.
    pub(super) fn player(cx: &App) -> Entity<player::Player> {
        cx.global::<Self>().player.clone()
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
        let player = {
            let services = cx.global_mut::<Self>();
            services.backend = Some(backend);
            services.player.clone()
        };
        player.update(cx, |player, _| player.connect(handle.clone()));
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
