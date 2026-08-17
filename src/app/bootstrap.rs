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
    let app = Application::new().with_assets(gpui_component_assets::Assets);
    // Clicking the Dock icon with no window open puts one back over the
    // services that kept playing in the meantime.
    app.on_reopen(|cx| {
        // AppKit can deliver this during launch, before the services exist.
        if cx.has_global::<services::AppServices>() && cx.windows().is_empty() {
            open_main_window(cx);
        }
    });
    app.run(move |cx: &mut App| {
        gpui_component::init(cx);
        cx.set_http_client(Arc::new(
            http::ImageHttpClient::new().expect("could not configure image HTTP client"),
        ));
        cx.on_action(|_: &Quit, cx| cx.quit());
        services::AppServices::init(cx, lifecycle, preferences_store, preferences);
        cx.bind_keys([
            KeyBinding::new("tab", Tab, None),
            KeyBinding::new("shift-tab", TabPrev, None),
            KeyBinding::new("cmd-k", OpenSearch, None),
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-w", CloseWindow, None),
            KeyBinding::new("escape", DismissOverlay, Some("Cadence")),
            playback_key_binding(),
        ]);
        // Without a menu bar, Cmd+Q is only deliverable through a window, so
        // closing the last one would leave no way to quit.
        cx.set_menus(vec![
            gpui::Menu {
                name: "Cadence".into(),
                items: vec![gpui::MenuItem::action("Quit Cadence", Quit)],
            },
            gpui::Menu {
                name: "Edit".into(),
                items: vec![
                    gpui::MenuItem::os_action("Cut", NoOp, gpui::OsAction::Cut),
                    gpui::MenuItem::os_action("Copy", NoOp, gpui::OsAction::Copy),
                    gpui::MenuItem::os_action("Paste", NoOp, gpui::OsAction::Paste),
                    gpui::MenuItem::os_action("Select All", NoOp, gpui::OsAction::SelectAll),
                ],
            },
            gpui::Menu {
                name: "Window".into(),
                items: vec![gpui::MenuItem::action("Close Window", CloseWindow)],
            },
        ]);
        watch_for_activations(cx);
        open_main_window(cx);
        cx.activate(true);
    });
}

/// Brings Cadence forward when another launch asks this instance to show
/// itself, opening a window again if the last one was closed.
fn watch_for_activations(cx: &mut App) {
    let activations = services::AppServices::activations(cx);
    cx.spawn(async move |cx| {
        while activations.recv().await.is_ok() {
            let updated = cx.update(|cx| {
                cx.activate(true);
                match cx.windows().first() {
                    Some(window) => {
                        let _ = window.update(cx, |_, window, _| window.activate_window());
                    }
                    None => open_main_window(cx),
                }
            });
            if updated.is_err() {
                break;
            }
        }
    })
    .detach();
}

/// Opens the Cadence window over the already-running services.
pub(super) fn open_main_window(cx: &mut App) {
    let backend = services::AppServices::backend(cx);
    let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
    let opened = cx.open_window(
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
            let cadence = cx.new(|cx| CadenceApp::new(window, cx, backend));
            services::AppServices::set_root(cadence.downgrade(), cx);
            cx.new(|cx| Root::new(cadence, window, cx))
        },
    );
    if let Err(error) = opened {
        log::error!("could not open the Cadence window: {error}");
    }
}

fn playback_key_binding() -> KeyBinding {
    KeyBinding::new("space", TogglePlayback, Some("Cadence && !Input"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_toggles_playback_except_in_text_inputs() {
        let keymap = gpui::Keymap::new(vec![playback_key_binding()]);
        let space = gpui::Keystroke::parse("space").unwrap();
        let cadence = gpui::KeyContext::try_from("Cadence").unwrap();
        let input = gpui::KeyContext::try_from("Input").unwrap();

        let (bindings, _) =
            keymap.bindings_for_input(std::slice::from_ref(&space), std::slice::from_ref(&cadence));
        assert_eq!(bindings.len(), 1);

        let (bindings, _) =
            keymap.bindings_for_input(std::slice::from_ref(&space), &[cadence, input]);
        assert!(bindings.is_empty());
    }
}
