use super::*;

pub(super) fn run() {
    let _ = env_logger::try_init();
    let lifecycle = match InstanceLifecycle::acquire().expect("could not initialize app lifecycle")
    {
        Instance::Primary(lifecycle) => lifecycle,
        Instance::Secondary => return,
    };
    let preferences_store = Store::open_default().ok();
    let preferences = preferences_store
        .as_ref()
        .and_then(|store| store.preferences().ok())
        .unwrap_or_default();
    Application::new()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            cx.set_http_client(Arc::new(
                http::ImageHttpClient::new().expect("could not configure image HTTP client"),
            ));
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.bind_keys([
                KeyBinding::new("tab", Tab, None),
                KeyBinding::new("shift-tab", TabPrev, None),
                KeyBinding::new("cmd-k", OpenSearch, None),
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("cmd-w", CloseWindow, None),
                KeyBinding::new("escape", DismissOverlay, Some("Cadence")),
                KeyBinding::new("space", TogglePlayback, Some("Cadence")),
            ]);
            let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(720.), px(600.))),
                    is_resizable: false,
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Cadence".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    let cadence = cx.new(|cx| {
                        CadenceApp::new(
                            window,
                            cx,
                            lifecycle.clone(),
                            preferences_store,
                            preferences,
                        )
                    });
                    cx.new(|cx| Root::new(cadence, window, cx))
                },
            )
            .expect("failed to open Cadence window");
            cx.activate(true);
        });
}
