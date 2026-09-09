use super::*;
use gpui_kit::{
    InputEvent as _, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent,
};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
enum Scene {
    Settings,
    SettingsAutoplayOff,
    SettingsMenu,
    Setup,
    SetupFilled,
    SearchEntered,
    Queue {
        open: bool,
        hover: bool,
        pressed: bool,
    },
}

fn scenes() -> Vec<(&'static str, Scene)> {
    vec![
        ("settings", Scene::Settings),
        ("settings-autoplay-off", Scene::SettingsAutoplayOff),
        ("settings-mascot-open", Scene::SettingsMenu),
        ("setup", Scene::Setup),
        ("setup-filled", Scene::SetupFilled),
        ("search-entered", Scene::SearchEntered),
        (
            "queue-closed",
            Scene::Queue {
                open: false,
                hover: false,
                pressed: false,
            },
        ),
        (
            "queue-hover",
            Scene::Queue {
                open: false,
                hover: true,
                pressed: false,
            },
        ),
        (
            "queue-pressed",
            Scene::Queue {
                open: false,
                hover: true,
                pressed: true,
            },
        ),
        (
            "queue-open",
            Scene::Queue {
                open: true,
                hover: false,
                pressed: false,
            },
        ),
        (
            "queue-open-hover",
            Scene::Queue {
                open: true,
                hover: true,
                pressed: false,
            },
        ),
        (
            "queue-open-pressed",
            Scene::Queue {
                open: true,
                hover: true,
                pressed: true,
            },
        ),
    ]
}

fn capture(scene: Scene, theme: ThemePreference, width: f32) -> image::RgbaImage {
    let mut cx = HeadlessAppContext::with_platform(
        gpui_kit::platform::current_platform(true).text_system(),
        Arc::new(assets::CadenceAssets),
        gpui_kit::platform::current_headless_renderer,
    );
    cx.update(|cx| {
        cx.set_reduce_motion(true);
        cx.text_system()
            .add_fonts(vec![
                Cow::Borrowed(include_bytes!(
                    "../../../tests/visual/fonts/Inter-Variable.ttf"
                )),
                Cow::Borrowed(include_bytes!(
                    "../../../tests/visual/fonts/Inter-Medium.ttf"
                )),
                Cow::Borrowed(include_bytes!(
                    "../../../tests/visual/fonts/Inter-SemiBold.ttf"
                )),
                Cow::Borrowed(include_bytes!("../../../tests/visual/fonts/Inter-Bold.ttf")),
            ])
            .expect("pinned Inter fonts");
    });
    let _backend = cx.update(|cx| initialize(cx, theme));
    let height = 820.;
    let handle = if matches!(scene, Scene::Setup | Scene::SetupFilled) {
        cx.update(|cx| {
            services::AppServices::session(cx).update(cx, |session, cx| {
                session.handle_backend_event(BackendEvent::SetupRequired, cx);
            });
        });
        cx.open_window(size(px(width), px(height)), |window, cx| {
            appearance::Appearance::attach(window, cx);
            let view = cx.new(|cx| onboarding::Onboarding::new(window, cx));
            if matches!(scene, Scene::SetupFilled) {
                view.update(cx, |view, cx| view.focus_setup_field(window, cx));
            }
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("setup window")
    } else {
        let (handle, workspace) = workspace(&mut cx, width, height);
        cx.update(|cx| {
            workspace.update(cx, |workspace, cx| match scene {
                Scene::Settings | Scene::SettingsAutoplayOff | Scene::SettingsMenu => {
                    workspace.open_settings(cx);
                    if matches!(scene, Scene::SettingsAutoplayOff) {
                        services::AppServices::set_autoplay(false, cx)
                            .expect("settings store")
                            .expect("save autoplay");
                        workspace.settings.update(cx, |_, cx| cx.notify());
                    }
                }
                Scene::Queue { open, .. } => {
                    workspace
                        .player_bar
                        .update(cx, |bar, cx| bar.set_queue_open(open, cx));
                    cx.notify();
                }
                _ => {}
            });
        });
        handle
    };
    cx.update_window(handle.into(), |_, window, _| window.activate_window())
        .expect("activate fixture window");
    settle(&mut cx, handle.into());
    match scene {
        Scene::SearchEntered => {
            key(&mut cx, handle, "cmd-k");
            for character in "Blue Train".chars() {
                key(&mut cx, handle, &character.to_string());
            }
        }
        Scene::SetupFilled => {
            for character in "0123456789abcdef0123456789abcdef".chars() {
                key(&mut cx, handle, &character.to_string());
            }
        }
        Scene::SettingsMenu => {
            let sidebar_width = if uses_compact_player_layout(width) {
                200.
            } else {
                232.
            };
            let content_width = (width - sidebar_width).min(760.);
            let position = point(
                px((width + sidebar_width + content_width) / 2. - 170.),
                px(480.),
            );
            cx.update_window(handle.into(), |_, window, cx| {
                window.dispatch_event(
                    MouseDownEvent {
                        button: MouseButton::Left,
                        position,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                        first_mouse: false,
                    }
                    .to_platform_input(),
                    cx,
                );
                window.dispatch_event(
                    MouseUpEvent {
                        button: MouseButton::Left,
                        position,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                    }
                    .to_platform_input(),
                    cx,
                );
            })
            .expect("open mascot selector");
            settle(&mut cx, handle.into());
        }
        _ => {}
    }
    if let Scene::Queue {
        hover: true,
        pressed,
        ..
    } = scene
    {
        let compact = uses_compact_player_layout(width);
        let controls_width = if compact { 88. } else { 216. };
        let position = point(px(width - 24. - controls_width + 20.), px(height - 48.));
        cx.update_window(handle.into(), |_, window, cx| {
            window.dispatch_event(
                MouseMoveEvent {
                    position,
                    pressed_button: None,
                    modifiers: Modifiers::default(),
                }
                .to_platform_input(),
                cx,
            );
            if pressed {
                window.dispatch_event(
                    MouseDownEvent {
                        button: MouseButton::Left,
                        position,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                        first_mouse: false,
                    }
                    .to_platform_input(),
                    cx,
                );
            }
        })
        .expect("queue pointer state");
        settle(&mut cx, handle.into());
    }
    cx.capture_screenshot(handle.into())
        .expect("native Metal screenshot")
}

fn key(cx: &mut HeadlessAppContext, handle: WindowHandle<Root>, key: &str) {
    let key = if key == " " { "space" } else { key };
    cx.update_window(handle.into(), |_, window, cx| {
        window.dispatch_keystroke(Keystroke::parse(key).expect("fixture key"), cx);
    })
    .expect("native keyboard input");
    settle(cx, handle.into());
}

fn compare(
    name: &str,
    actual: &image::RgbaImage,
    expected: &image::RgbaImage,
    artifacts: &Path,
) -> bool {
    if expected == actual {
        return true;
    }
    std::fs::create_dir_all(artifacts).expect("create visual artifacts directory");
    actual
        .save(artifacts.join(format!("{name}-actual.png")))
        .expect("actual image");
    expected
        .save(artifacts.join(format!("{name}-expected.png")))
        .expect("expected image");
    let difference = image::RgbaImage::from_fn(
        actual.width().max(expected.width()),
        actual.height().max(expected.height()),
        |x, y| {
            if actual.get_pixel_checked(x, y) == expected.get_pixel_checked(x, y) {
                image::Rgba([0, 0, 0, 255])
            } else {
                image::Rgba([255, 0, 128, 255])
            }
        },
    );
    difference
        .save(artifacts.join(format!("{name}-diff.png")))
        .expect("difference image");
    eprintln!("pixel mismatch: {name}");
    false
}

pub(crate) fn run() {
    let baseline = std::env::var_os("CADENCE_UI_BASELINE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/visual/baseline"));
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/ui-artifacts");
    if artifacts.exists() {
        std::fs::remove_dir_all(&artifacts).expect("clear previous visual artifacts");
    }
    let record = std::env::args().any(|argument| argument == "--record");
    if record {
        assert!(
            std::env::var_os("CADENCE_UI_BASELINE").is_some(),
            "Recording requires an explicit CADENCE_UI_BASELINE directory"
        );
        std::fs::create_dir_all(&baseline).expect("create baseline directory");
    }
    let mut mismatches = 0;
    let mut count = 0;
    for (theme_name, theme) in [
        ("light", ThemePreference::Light),
        ("dark", ThemePreference::Dark),
    ] {
        for (layout, width) in [("regular", 1280.), ("compact", 900.)] {
            for (name, scene) in scenes() {
                let name = format!("{theme_name}-{layout}-{name}");
                let actual = capture(scene, theme, width);
                let repeated = capture(scene, theme, width);
                assert!(
                    compare(&format!("{name}-repeat"), &repeated, &actual, &artifacts),
                    "nonrepeatable rendering: {name}"
                );
                if record {
                    actual
                        .save(baseline.join(format!("{name}.png")))
                        .expect("record baseline");
                } else {
                    let expected_path = baseline.join(format!("{name}.png"));
                    match image::open(&expected_path) {
                        Ok(expected) => {
                            if !compare(&name, &actual, &expected.to_rgba8(), &artifacts) {
                                mismatches += 1;
                            }
                        }
                        Err(error) => {
                            std::fs::create_dir_all(&artifacts)
                                .expect("create artifacts directory");
                            actual
                                .save(artifacts.join(format!("{name}-actual.png")))
                                .expect("actual image");
                            eprintln!("read {}: {error}", expected_path.display());
                            mismatches += 1;
                        }
                    }
                }
                count += 1;
            }
        }
    }
    assert_eq!(
        mismatches,
        0,
        "native visual regressions, see {}",
        artifacts.display()
    );
    println!("Native visual checks: {count} repeatable cases, {mismatches} mismatches");
}
