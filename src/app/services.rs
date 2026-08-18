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
    brand_mark: Arc<gpui::Image>,
    media_controls: Option<media_controls::SystemMediaControls>,
    /// The window currently showing these services, if one is open.
    root: Option<gpui::WeakEntity<CadenceApp>>,
    /// Drains backend events for the whole process, not just for a window.
    event_pump: Option<gpui::Task<()>>,
    lifecycle: Arc<InstanceLifecycle>,
    store: Option<Store>,
    /// The live preference values, so a window opened later starts from what
    /// the listener last chose rather than from what was on disk at launch.
    preferences: AppPreferences,
}

impl gpui::Global for AppServices {}

impl AppServices {
    pub(super) fn init(
        cx: &mut App,
        lifecycle: Arc<InstanceLifecycle>,
        store: Option<Store>,
        preferences: AppPreferences,
    ) -> BackendHandle {
        let (backend, events) = Backend::start();
        let handle = backend.handle();
        let player = cx.new(|_| player::Player::new(handle.clone()));
        let session = cx.new(|_| session::Session::new(handle.clone()));
        let library = cx.new(|_| library::Library::new(handle.clone()));
        let image_cache = image_cache::BoundedImageCache::new(cx);
        let brand_mark = Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            include_bytes!("../../assets/cadence-mark.png").to_vec(),
        ));
        let player_for_media = player.clone();
        // Keep the system's now-playing panel in step with the player.
        cx.observe(&player, |player, cx| {
            let mut services = cx.remove_global::<Self>();
            if let Some(controls) = services.media_controls.as_mut() {
                controls.sync(player.read(cx));
            }
            cx.set_global(services);
        })
        .detach();
        cx.on_app_quit(|cx| {
            Self::shutdown(cx);
            async {}
        })
        .detach();
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                // The window owned the live position; persist it while the
                // services keep playing without one.
                Self::player(cx).read(cx).save_position();
            }
        })
        .detach();
        let media_controls = media_controls::SystemMediaControls::attach(player_for_media, cx);
        cx.set_global(Self {
            backend: Some(backend),
            player,
            session,
            library,
            image_cache,
            brand_mark,
            media_controls,
            root: None,
            event_pump: None,
            lifecycle,
            store,
            preferences,
        });
        Self::pump(events, cx);
        handle
    }

    pub(super) fn backend(cx: &App) -> BackendHandle {
        cx.global::<Self>()
            .backend
            .as_ref()
            .expect("services are shut down")
            .handle()
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

    /// The Cadence mark, drawn by both the sidebar and the setup screen.
    pub(super) fn brand_mark(cx: &App) -> Arc<gpui::Image> {
        cx.global::<Self>().brand_mark.clone()
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

    pub(super) fn preferences(cx: &App) -> AppPreferences {
        cx.global::<Self>().preferences
    }

    pub(super) fn set_theme_preference(
        preference: ThemePreference,
        cx: &mut App,
    ) -> Option<anyhow::Result<()>> {
        let services = cx.global_mut::<Self>();
        services.preferences.theme = preference;
        services
            .store
            .as_mut()
            .map(|store| store.set_theme_preference(preference))
    }

    pub(super) fn set_sidebar_collapsed(
        collapsed: bool,
        cx: &mut App,
    ) -> Option<anyhow::Result<()>> {
        let services = cx.global_mut::<Self>();
        services.preferences.sidebar_collapsed = collapsed;
        services
            .store
            .as_mut()
            .map(|store| store.set_sidebar_collapsed(collapsed))
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

    /// Restarts the worker after a fatal failure. Handles already handed out
    /// keep working: `Backend::restart` redirects them at the new worker.
    pub(super) fn restart(cx: &mut App) {
        let handle = Self::backend(cx);
        let (backend, events) = Backend::restart(&handle);
        let session = {
            let services = cx.global_mut::<Self>();
            services.backend = Some(backend);
            services.session.clone()
        };
        session.update(cx, |session, cx| session.restarted(cx));
        Self::pump(events, cx);
    }

    /// Saves the live position and stops the worker thread. The process exits
    /// straight after `applicationWillTerminate:`, so nothing else will.
    fn shutdown(cx: &mut App) {
        let position_ms = Self::player(cx).read(cx).position_snapshot();
        // Stop the worker before writing, both because its shutdown drops any
        // command still queued behind it, and so it cannot write a later
        // position over this one on its way out.
        let backend = cx.global_mut::<Self>().backend.take();
        drop(backend);
        let Some(position_ms) = position_ms else {
            return;
        };
        let saved = cx
            .global_mut::<Self>()
            .store
            .as_mut()
            .map(|store| store.update_playback_position(position_ms));
        if let Some(Err(error)) = saved {
            log::error!("could not save the playback position: {error}");
        }
    }
}
