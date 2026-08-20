use super::*;

/// Which windows a session state calls for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DesiredWindows {
    OnboardingOnly,
    MainOnly,
    MainWithOnboarding,
}

/// Before the session has ever been ready, onboarding stands alone. Once the
/// main window has been the listener's home, losing the session brings the
/// sign-in window up over it instead of tearing it down.
pub(super) fn desired_windows(state: ConnectionState, has_been_ready: bool) -> DesiredWindows {
    match state {
        ConnectionState::Ready => DesiredWindows::MainOnly,
        ConnectionState::Starting
        | ConnectionState::Failed
        | ConnectionState::SetupRequired
        | ConnectionState::AuthorizationRequired
        | ConnectionState::Connecting => {
            if has_been_ready {
                DesiredWindows::MainWithOnboarding
            } else {
                DesiredWindows::OnboardingOnly
            }
        }
    }
}

/// Reconciles the open windows after a session change. Acts only when the
/// connection state actually moved, so a window the listener closed stays
/// closed until the next transition.
pub(super) fn sync_windows(cx: &mut App) {
    let session = services::AppServices::session(cx);
    let state = *session.read(cx).state();
    if !services::AppServices::note_connection_state(state, cx) {
        return;
    }
    match desired_windows(state, services::AppServices::has_been_ready(cx)) {
        DesiredWindows::MainOnly => {
            if services::AppServices::main_window(cx).is_none() {
                open_main_window(cx);
            }
            unlock_and_close_onboarding(cx);
        }
        // Main stays however the listener left it; only sign-in is required.
        DesiredWindows::MainWithOnboarding => {
            ensure_onboarding_window(cx);
            lock_onboarding_over_main(cx);
        }
        DesiredWindows::OnboardingOnly => {
            ensure_onboarding_window(cx);
            close_window(services::AppServices::take_main_window(cx), cx);
        }
    }
}

/// Picks the first window from what the store says about credentials, before
/// the backend has confirmed anything. A wrong guess is corrected by the
/// first session state change.
pub(super) fn open_initial_window(credentials_expected: bool, cx: &mut App) {
    if credentials_expected {
        open_main_window(cx);
    } else {
        ensure_onboarding_window(cx);
    }
}

/// Brings Cadence forward: activates an open window (sign-in first, since it
/// is the one asking something of the listener), or opens whichever window
/// the current state calls for.
pub(super) fn show_app_window(cx: &mut App) {
    for handle in [
        services::AppServices::onboarding_window(cx),
        services::AppServices::main_window(cx),
    ]
    .into_iter()
    .flatten()
    {
        if handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
        {
            return;
        }
    }
    let state = *services::AppServices::session(cx).read(cx).state();
    match desired_windows(state, services::AppServices::has_been_ready(cx)) {
        DesiredWindows::MainOnly => open_main_window(cx),
        DesiredWindows::OnboardingOnly | DesiredWindows::MainWithOnboarding => {
            ensure_onboarding_window(cx)
        }
    }
}

/// Opens the resizable main window over the already-running services.
pub(super) fn open_main_window(cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(720.), px(600.))),
            is_resizable: true,
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Cadence".into()),
                appears_transparent: true,
                ..Default::default()
            }),
            ..Default::default()
        },
        move |window, cx| {
            let cadence = cx.new(|cx| Workspace::new(window, cx));
            services::AppServices::set_root(cadence.downgrade(), cx);
            cx.new(|cx| Root::new(cadence, window, cx))
        },
    );
    match opened {
        Ok(handle) => services::AppServices::set_main_window(Some(handle.into()), cx),
        Err(error) => log::error!("could not open the Cadence window: {error}"),
    }
}

/// Opens the fixed-size sign-in window, or brings the open one forward. The
/// size fits the onboarding layout (420px rail + content) above the compact
/// breakpoint; the window is not resizable, so that is the only layout.
pub(super) fn ensure_onboarding_window(cx: &mut App) {
    if let Some(handle) = services::AppServices::onboarding_window(cx) {
        let _ = handle.update(cx, |_, window, _| window.activate_window());
        return;
    }
    let bounds = onboarding_bounds(cx);
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            is_resizable: false,
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Cadence".into()),
                appears_transparent: true,
                ..Default::default()
            }),
            ..Default::default()
        },
        |window, cx| {
            let onboarding = cx.new(|cx| OnboardingWindow::new(window, cx));
            cx.new(|cx| Root::new(onboarding, window, cx))
        },
    );
    match opened {
        Ok(handle) => services::AppServices::set_onboarding_window(Some(handle.into()), cx),
        Err(error) => log::error!("could not open the sign-in window: {error}"),
    }
}

/// Centered over the main window when one is open, otherwise on the display.
fn onboarding_bounds(cx: &mut App) -> Bounds<Pixels> {
    let onboarding_size = size(px(1140.), px(720.));
    let main_bounds = services::AppServices::main_window(cx)
        .and_then(|handle| handle.update(cx, |_, window, _| window.bounds()).ok());
    match main_bounds {
        Some(main) => Bounds::centered_at(main.center(), onboarding_size),
        None => Bounds::centered(None, onboarding_size, cx),
    }
}

fn close_window(handle: Option<gpui::AnyWindowHandle>, cx: &mut App) {
    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
}

/// Pins the sign-in window over main as one unit: the pair moves together,
/// the sign-in window cannot fall behind main, and neither closes separately.
fn lock_onboarding_over_main(cx: &mut App) {
    #[cfg(target_os = "macos")]
    {
        let parent = mac_window(services::AppServices::main_window(cx), cx);
        let child = mac_window(services::AppServices::onboarding_window(cx), cx);
        if let (Some(parent), Some(child)) = (parent, child) {
            modal::attach_above(parent, child);
            modal::set_closable(parent, false);
            modal::set_closable(child, false);
        }
    }
}

/// Releases the pair and closes the sign-in window.
fn unlock_and_close_onboarding(cx: &mut App) {
    let onboarding = services::AppServices::take_onboarding_window(cx);
    #[cfg(target_os = "macos")]
    {
        let parent = mac_window(services::AppServices::main_window(cx), cx);
        let child = mac_window(onboarding, cx);
        if let (Some(parent), Some(child)) = (parent, child) {
            modal::detach(parent, child);
        }
        if let Some(parent) = parent {
            modal::set_closable(parent, true);
        }
    }
    close_window(onboarding, cx);
}

#[cfg(target_os = "macos")]
fn mac_window(
    handle: Option<gpui::AnyWindowHandle>,
    cx: &mut App,
) -> Option<*mut objc::runtime::Object> {
    handle?
        .update(cx, |_, window, _| modal::ns_window(window))
        .ok()
        .flatten()
}

/// gpui 0.2 has no window-level modality (WindowKind::Floating is a no-op on
/// macOS), so the sign-in window is pinned over main with AppKit directly.
/// Re-verify these calls on a gpui upgrade.
// objc 0.2's macros expand a stale `cfg(feature = "cargo-clippy")` check.
#[allow(unexpected_cfgs)]
#[cfg(target_os = "macos")]
mod modal {
    use objc::{msg_send, runtime::Object, sel, sel_impl};
    use raw_window_handle::RawWindowHandle;

    const CLOSABLE: u64 = 1 << 1;
    const ABOVE: i64 = 1;

    pub(super) fn ns_window(window: &gpui::Window) -> Option<*mut Object> {
        let handle = raw_window_handle::HasWindowHandle::window_handle(window).ok()?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return None;
        };
        let view = handle.ns_view.as_ptr() as *mut Object;
        Some(unsafe { msg_send![view, window] })
    }

    /// Attaches `child` so it moves with `parent` and always stays above it.
    pub(super) fn attach_above(parent: *mut Object, child: *mut Object) {
        unsafe {
            let _: () = msg_send![parent, addChildWindow: child ordered: ABOVE];
        }
    }

    pub(super) fn detach(parent: *mut Object, child: *mut Object) {
        unsafe {
            let _: () = msg_send![parent, removeChildWindow: child];
        }
    }

    pub(super) fn set_closable(window: *mut Object, closable: bool) {
        unsafe {
            let mask: u64 = msg_send![window, styleMask];
            let mask = if closable {
                mask | CLOSABLE
            } else {
                mask & !CLOSABLE
            };
            let _: () = msg_send![window, setStyleMask: mask];
        }
    }
}

/// The root of the sign-in window: a fresh Onboarding view per window (its
/// input wiring is bound to the window it was created in) plus the session
/// error and notice plumbing the workspace used to provide.
pub(super) struct OnboardingWindow {
    onboarding: Entity<onboarding::Onboarding>,
    session: Entity<session::Session>,
    last_error: Option<String>,
    action_notice: Option<String>,
    _appearance_subscription: Subscription,
}

impl OnboardingWindow {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        appearance::Appearance::attach(window, cx);
        let appearance_subscription = cx.observe_window_appearance(window, |_, window, cx| {
            if appearance::Appearance::follow_system(window, cx) {
                cx.notify();
            }
        });
        let session = services::AppServices::session(cx);
        cx.subscribe(&session, |this, _, event: &session::SessionEvent, cx| {
            match event {
                session::SessionEvent::Failed(error) => this.last_error = Some(error.clone()),
                session::SessionEvent::Ready => this.last_error = None,
                session::SessionEvent::Notice(notice) => this.action_notice = Some(notice.clone()),
                session::SessionEvent::Restarted | session::SessionEvent::LoggedOut => {}
            }
            cx.notify();
        })
        .detach();
        let onboarding = cx.new(|cx| onboarding::Onboarding::new(window, cx));
        cx.subscribe(
            &onboarding,
            |this, _, event: &onboarding::OnboardingEvent, cx| {
                match event {
                    onboarding::OnboardingEvent::Authenticate => {
                        this.session
                            .update(cx, |session, cx| session.authenticate(cx));
                    }
                    onboarding::OnboardingEvent::ChangeSpotifyApp => {
                        this.session
                            .update(cx, |session, cx| session.request_app_change(cx));
                    }
                    onboarding::OnboardingEvent::RetryBackend => {
                        services::AppServices::restart(cx);
                        this.last_error = None;
                    }
                    onboarding::OnboardingEvent::DismissOverlay => {
                        this.session
                            .update(cx, |session, cx| session.cancel_app_change(cx));
                    }
                    onboarding::OnboardingEvent::ClearError => this.last_error = None,
                    onboarding::OnboardingEvent::Notice(notice) => {
                        this.action_notice = Some(notice.clone())
                    }
                }
                cx.notify();
            },
        )
        .detach();
        // The failure that opened this window was emitted before it existed;
        // seed it from the session rather than waiting for the next one.
        let last_error = session.read(cx).last_failure().cloned();
        Self {
            onboarding,
            session,
            last_error,
            action_notice: None,
            _appearance_subscription: appearance_subscription,
        }
    }
}

impl Render for OnboardingWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::Appearance::palette(cx);
        let error = self.last_error.clone();
        self.onboarding.update(cx, |onboarding, cx| {
            onboarding.show_error(error, cx);
            onboarding.focus_setup_field(window, cx);
        });
        let app_change_open = self.session.read(cx).app_change_confirmation_open();
        let notice = self.action_notice.clone().map(|message| {
            components::action_notice_banner(
                palette,
                message,
                cx.listener(|this, _, _, cx| {
                    this.action_notice = None;
                    cx.notify();
                }),
            )
        });
        div()
            .id("onboarding-window")
            .size_full()
            .relative()
            .on_action(cx.listener(|_, _: &CloseWindow, window, cx| {
                // Locked over the main window, the pair only closes together.
                if services::AppServices::main_window(cx).is_none() {
                    window.remove_window();
                }
            }))
            .child(self.onboarding.clone())
            .when_some(notice, |root, notice| root.child(notice))
            .when(app_change_open, |root| {
                root.child(deferred(chrome::spotify_app_change_confirmation(
                    palette,
                    self.session.read(cx).profile().is_some(),
                    cx.listener(|this, _, _, cx| {
                        this.session
                            .update(cx, |session, cx| session.cancel_app_change(cx))
                    }),
                    cx.listener(|this, _, _, cx| {
                        this.session
                            .update(cx, |session, cx| session.confirm_app_change(cx))
                    }),
                )))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOT_READY: [ConnectionState; 5] = [
        ConnectionState::Starting,
        ConnectionState::Failed,
        ConnectionState::SetupRequired,
        ConnectionState::AuthorizationRequired,
        ConnectionState::Connecting,
    ];

    #[test]
    fn before_first_ready_only_the_onboarding_window_exists() {
        for state in NOT_READY {
            assert_eq!(
                desired_windows(state, false),
                DesiredWindows::OnboardingOnly,
                "{state:?}"
            );
        }
    }

    #[test]
    fn ready_shows_only_the_main_window() {
        assert_eq!(
            desired_windows(ConnectionState::Ready, false),
            DesiredWindows::MainOnly
        );
        assert_eq!(
            desired_windows(ConnectionState::Ready, true),
            DesiredWindows::MainOnly
        );
    }

    #[test]
    fn losing_the_session_after_ready_keeps_main_and_adds_sign_in() {
        for state in NOT_READY {
            assert_eq!(
                desired_windows(state, true),
                DesiredWindows::MainWithOnboarding,
                "{state:?}"
            );
        }
    }
}
