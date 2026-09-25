use super::*;

pub(super) fn run() {
    let _ = env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .try_init();
    log::info!("startup: process started");
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
    let credentials_expected = preferences_store
        .as_ref()
        .is_some_and(stored_credentials_expected);
    let app = gpui_kit::application().with_assets(assets::CadenceAssets);
    // Clicking the Dock icon with no window open puts one back over the
    // services that kept playing in the meantime.
    app.on_reopen(|cx| {
        // AppKit can deliver this during launch, before the services exist.
        if cx.has_global::<services::AppServices>() {
            windows::show_app_window(cx);
        }
    });
    app.run(move |cx: &mut App| {
        log::info!("startup: gpui application running");
        gpui_kit::init(cx);
        cx.set_http_client(Arc::new(
            http::ImageHttpClient::new().expect("could not configure image HTTP client"),
        ));
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_action(|_: &CheckForUpdates, cx| updater::check_for_updates(cx));
        services::AppServices::init(cx, lifecycle, preferences_store, preferences);
        bind_keys(cx);
        let updates_available = updater::start(cx);
        // Without a menu bar, Cmd+Q is only deliverable through a window, so
        // closing the last one would leave no way to quit.
        cx.set_menus(menus(updates_available));
        watch_for_activations(cx);
        windows::open_initial_window(credentials_expected, cx);
        log::info!("startup: first window opened");
        cx.activate(true);
    });
}

fn menus(updates_available: bool) -> Vec<gpui_kit::Menu> {
    vec![
        gpui_kit::Menu {
            name: "Cadence".into(),
            items: app_menu_items(updates_available),
            disabled: false,
        },
        gpui_kit::Menu {
            name: "Edit".into(),
            items: edit_menu_items(),
            disabled: false,
        },
        gpui_kit::Menu {
            name: "Window".into(),
            items: vec![gpui_kit::MenuItem::action("Close Window", CloseWindow)],
            disabled: false,
        },
    ]
}

/// The update check only appears in bundles that can update themselves.
fn app_menu_items(updates_available: bool) -> Vec<gpui_kit::MenuItem> {
    let mut items = Vec::new();
    if updates_available {
        items.push(gpui_kit::MenuItem::action(
            "Check for Updates…",
            CheckForUpdates,
        ));
        items.push(gpui_kit::MenuItem::separator());
    }
    items.push(gpui_kit::MenuItem::action("Quit Cadence", Quit));
    items
}

/// The focused text field's own commands, so the menu shows the field's
/// shortcuts and is enabled only while a field can take them.
fn edit_menu_items() -> Vec<gpui_kit::MenuItem> {
    use gpui_kit::OsAction;
    use gpui_kit::component::input;
    vec![
        gpui_kit::MenuItem::os_action("Cut", input::Cut, OsAction::Cut),
        gpui_kit::MenuItem::os_action("Copy", input::Copy, OsAction::Copy),
        gpui_kit::MenuItem::os_action("Paste", input::Paste, OsAction::Paste),
        gpui_kit::MenuItem::os_action("Select All", input::SelectAll, OsAction::SelectAll),
    ]
}

/// Whether the store says a signed-in session should come straight up: a
/// client id is configured and the OAuth credentials were not invalidated.
/// The backend has the final word; a wrong guess swaps the windows.
fn stored_credentials_expected(store: &Store) -> bool {
    let configured = std::env::var("SPOTIFY_CLIENT_ID").is_ok()
        || matches!(store.spotify_client_id(), Ok(Some(_)));
    configured
        && !store
            .spotify_oauth_credentials_invalidated()
            .unwrap_or(true)
}

/// Brings Cadence forward when another launch asks this instance to show
/// itself, opening a window again if the last one was closed.
fn watch_for_activations(cx: &mut App) {
    let activations = services::AppServices::activations(cx);
    cx.spawn(async move |cx| {
        while activations.recv().await.is_ok() {
            cx.update(|cx| {
                cx.activate(true);
                windows::show_app_window(cx);
            });
        }
    })
    .detach();
}

pub(super) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-k", OpenSearch, Some(WORKSPACE_KEY_CONTEXT)),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("escape", DismissOverlay, Some(WORKSPACE_KEY_CONTEXT)),
        KeyBinding::new("escape", DismissOverlay, Some(ONBOARDING_KEY_CONTEXT)),
        KeyBinding::new("left", SeekBackward, Some(SCRUBBER_KEY_CONTEXT)),
        KeyBinding::new("right", SeekForward, Some(SCRUBBER_KEY_CONTEXT)),
        KeyBinding::new("pagedown", SeekBackwardLarge, Some(SCRUBBER_KEY_CONTEXT)),
        KeyBinding::new("pageup", SeekForwardLarge, Some(SCRUBBER_KEY_CONTEXT)),
        KeyBinding::new("home", SeekToStart, Some(SCRUBBER_KEY_CONTEXT)),
        KeyBinding::new("end", SeekToEnd, Some(SCRUBBER_KEY_CONTEXT)),
        playback_key_binding(),
    ]);
}

fn playback_key_binding() -> KeyBinding {
    let context =
        format!("{WORKSPACE_KEY_CONTEXT} && !{INPUT_KEY_CONTEXT} && !{CONTROL_KEY_CONTEXT}");
    KeyBinding::new("space", TogglePlayback, Some(&context))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_toggles_playback_except_in_text_inputs_and_focused_controls() {
        let keymap = gpui_kit::Keymap::new(vec![playback_key_binding()]);
        let space = gpui_kit::Keystroke::parse("space").unwrap();
        let cadence = gpui_kit::KeyContext::try_from(WORKSPACE_KEY_CONTEXT).unwrap();
        let input = gpui_kit::KeyContext::try_from(INPUT_KEY_CONTEXT).unwrap();
        let control = gpui_kit::KeyContext::try_from(CONTROL_KEY_CONTEXT).unwrap();

        let (bindings, _) =
            keymap.bindings_for_input(std::slice::from_ref(&space), std::slice::from_ref(&cadence));
        assert_eq!(bindings.len(), 1);

        let (bindings, _) =
            keymap.bindings_for_input(std::slice::from_ref(&space), &[cadence.clone(), input]);
        assert!(bindings.is_empty());

        let (bindings, _) =
            keymap.bindings_for_input(std::slice::from_ref(&space), &[cadence, control]);
        assert!(bindings.is_empty());
    }

    #[test]
    fn every_edit_menu_item_is_an_input_action_with_a_key_binding() {
        let mut cx = gpui_kit::HeadlessAppContext::new(Arc::new(gpui_kit::NoopTextSystem));
        cx.update(|cx| {
            gpui_kit::init(cx);
            let keymap = cx.key_bindings();
            let keymap = keymap.borrow();
            for item in edit_menu_items() {
                let gpui_kit::MenuItem::Action { action, .. } = item else {
                    panic!("the Edit menu holds only actions");
                };
                assert!(action.name().starts_with("input::"), "{}", action.name());
                let bound = keymap.bindings_for_action(action.as_ref()).next().is_some();
                assert!(bound, "{} has no shortcut", action.name());
            }
        });
    }
}
