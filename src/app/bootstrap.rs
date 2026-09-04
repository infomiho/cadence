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
    let app = gpui_platform::application().with_assets(gpui_component_assets::Assets);
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
                disabled: false,
            },
            gpui::Menu {
                name: "Edit".into(),
                items: vec![
                    gpui::MenuItem::os_action("Cut", NoOp, gpui::OsAction::Cut),
                    gpui::MenuItem::os_action("Copy", NoOp, gpui::OsAction::Copy),
                    gpui::MenuItem::os_action("Paste", NoOp, gpui::OsAction::Paste),
                    gpui::MenuItem::os_action("Select All", NoOp, gpui::OsAction::SelectAll),
                ],
                disabled: false,
            },
            gpui::Menu {
                name: "Window".into(),
                items: vec![gpui::MenuItem::action("Close Window", CloseWindow)],
                disabled: false,
            },
        ]);
        watch_for_activations(cx);
        windows::open_initial_window(credentials_expected, cx);
        log::info!("startup: first window opened");
        cx.activate(true);
    });
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
