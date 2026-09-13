//! In-app updates through the Sparkle framework that `package-app.sh` embeds.
//!
//! Sparkle only runs in a release build whose bundle carries the framework
//! and a feed URL. Debug builds and the ad-hoc bundles from `run-signed.sh`
//! and an unsigned packaging run stay inert, so a development build never
//! offers to replace itself with a release. The update windows are Sparkle's
//! own. A small delegate mirrors what Sparkle found into `UpdateStatus` so the
//! settings page can show it.

// objc 0.2's macros expand a stale `cfg(feature = "cargo-clippy")` check.
#![allow(unexpected_cfgs)]

use super::*;

use std::ffi::{CStr, CString, c_char};
use std::sync::OnceLock;

use objc::declare::ClassDecl;
use objc::runtime::{BOOL, Class, NO, Object, Protocol, Sel, YES};
use objc::{class, msg_send, sel, sel_impl};

/// The choice in Sparkle's window that hides this version for good.
const USER_UPDATE_CHOICE_SKIP: isize = 0;

/// What Sparkle last reported, watched by the settings page.
#[derive(Default)]
pub(super) struct UpdateStatus {
    pub(super) check: UpdateCheck,
}

/// The outcome of Sparkle's most recent check, by display version.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum UpdateCheck {
    #[default]
    Unknown,
    UpToDate,
    Available(String),
    Skipped(String),
}

/// Sparkle's standard updater controller and its delegate, kept alive for
/// the whole process.
pub(super) struct Updater {
    controller: *mut Object,
    _delegate: *mut Object,
    status: Entity<UpdateStatus>,
}

impl gpui_kit::Global for Updater {}

enum UpdateEvent {
    Found(String),
    NotFound,
    Skipped(String),
}

static EVENTS: OnceLock<async_channel::Sender<UpdateEvent>> = OnceLock::new();

/// Starts Sparkle when this bundle is meant to update itself and records
/// the controller so the menu and settings can reach it.
pub(super) fn start(cx: &mut App) -> bool {
    if !should_start(cfg!(debug_assertions), feed_url().is_some()) {
        log::info!("startup: updater inert for this build");
        return false;
    }
    if !load_framework() {
        log::warn!("startup: Sparkle framework missing from the bundle");
        return false;
    }
    let Some(controller_class) = Class::get("SPUStandardUpdaterController") else {
        log::warn!("startup: Sparkle framework loaded without its controller");
        return false;
    };
    let (sender, receiver) = async_channel::unbounded();
    if EVENTS.set(sender).is_err() {
        log::warn!("startup: updater already started");
        return false;
    }
    let status = cx.new(|_| UpdateStatus::default());
    cx.spawn({
        let status = status.clone();
        async move |cx| {
            while let Ok(event) = receiver.recv().await {
                cx.update(|cx| {
                    status.update(cx, |status, cx| {
                        status.check = match event {
                            UpdateEvent::Found(version) => UpdateCheck::Available(version),
                            UpdateEvent::NotFound => UpdateCheck::UpToDate,
                            UpdateEvent::Skipped(version) => UpdateCheck::Skipped(version),
                        };
                        cx.notify();
                    })
                });
            }
        }
    })
    .detach();
    let delegate: *mut Object = unsafe { msg_send![delegate_class(), new] };
    let controller: *mut Object = unsafe {
        let nil: *mut Object = std::ptr::null_mut();
        let controller: *mut Object = msg_send![controller_class, alloc];
        msg_send![
            controller,
            initWithStartingUpdater: YES
            updaterDelegate: delegate
            userDriverDelegate: nil
        ]
    };
    if controller.is_null() {
        log::warn!("startup: Sparkle refused to start");
        return false;
    }
    cx.set_global(Updater {
        controller,
        _delegate: delegate,
        status,
    });
    log::info!("startup: updater started");
    true
}

/// Runs a user-initiated update check with Sparkle's own progress windows.
pub(super) fn check_for_updates(cx: &mut App) {
    let Some(updater) = cx.try_global::<Updater>() else {
        return;
    };
    let nil: *mut Object = std::ptr::null_mut();
    let _: () = unsafe { msg_send![updater.controller, checkForUpdates: nil] };
}

/// What Sparkle found, or `None` when this build does not update itself.
pub(super) fn status(cx: &App) -> Option<Entity<UpdateStatus>> {
    cx.try_global::<Updater>()
        .map(|updater| updater.status.clone())
}

/// Whether Sparkle looks for updates on its own, or `None` when this build
/// does not update itself. Sparkle persists the choice.
pub(super) fn automatic_checks(cx: &App) -> Option<bool> {
    let updater = cx.try_global::<Updater>()?;
    let enabled: BOOL =
        unsafe { msg_send![sparkle_updater(updater), automaticallyChecksForUpdates] };
    Some(enabled == YES)
}

pub(super) fn set_automatic_checks(enabled: bool, cx: &mut App) {
    let Some(updater) = cx.try_global::<Updater>() else {
        return;
    };
    let flag = if enabled { YES } else { NO };
    let _: () =
        unsafe { msg_send![sparkle_updater(updater), setAutomaticallyChecksForUpdates: flag] };
}

fn sparkle_updater(updater: &Updater) -> *mut Object {
    unsafe { msg_send![updater.controller, updater] }
}

fn should_start(debug_build: bool, has_feed: bool) -> bool {
    !debug_build && has_feed
}

fn feed_url() -> Option<String> {
    unsafe {
        let bundle: *mut Object = msg_send![class!(NSBundle), mainBundle];
        let key = nsstring("SUFeedURL");
        let value: *mut Object = msg_send![bundle, objectForInfoDictionaryKey: key];
        string(value)
    }
}

fn load_framework() -> bool {
    unsafe {
        let main_bundle: *mut Object = msg_send![class!(NSBundle), mainBundle];
        let frameworks: *mut Object = msg_send![main_bundle, privateFrameworksPath];
        if frameworks.is_null() {
            return false;
        }
        let name = nsstring("Sparkle.framework");
        let path: *mut Object = msg_send![frameworks, stringByAppendingPathComponent: name];
        let bundle: *mut Object = msg_send![class!(NSBundle), bundleWithPath: path];
        if bundle.is_null() {
            return false;
        }
        let loaded: BOOL = msg_send![bundle, load];
        loaded == YES
    }
}

/// The `SPUUpdaterDelegate` that reports Sparkle's findings back to the app.
fn delegate_class() -> &'static Class {
    if let Some(class) = Class::get("CadenceUpdaterDelegate") {
        return class;
    }
    let mut declaration = ClassDecl::new("CadenceUpdaterDelegate", class!(NSObject))
        .expect("the delegate class name is unused");
    if let Some(protocol) = Protocol::get("SPUUpdaterDelegate") {
        declaration.add_protocol(protocol);
    }
    unsafe {
        declaration.add_method(
            sel!(updater:didFindValidUpdate:),
            did_find_valid_update as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );
        declaration.add_method(
            sel!(updaterDidNotFindUpdate:),
            did_not_find_update as extern "C" fn(&Object, Sel, *mut Object),
        );
        declaration.add_method(
            sel!(updater:userDidMakeChoice:forUpdate:state:),
            user_did_make_choice
                as extern "C" fn(&Object, Sel, *mut Object, isize, *mut Object, *mut Object),
        );
    }
    declaration.register()
}

extern "C" fn did_find_valid_update(_: &Object, _: Sel, _: *mut Object, item: *mut Object) {
    if let Some(version) = display_version(item) {
        report(UpdateEvent::Found(version));
    }
}

extern "C" fn did_not_find_update(_: &Object, _: Sel, _: *mut Object) {
    report(UpdateEvent::NotFound);
}

extern "C" fn user_did_make_choice(
    _: &Object,
    _: Sel,
    _: *mut Object,
    choice: isize,
    item: *mut Object,
    _: *mut Object,
) {
    if choice != USER_UPDATE_CHOICE_SKIP {
        return;
    }
    if let Some(version) = display_version(item) {
        report(UpdateEvent::Skipped(version));
    }
}

fn display_version(item: *mut Object) -> Option<String> {
    if item.is_null() {
        return None;
    }
    unsafe {
        let version: *mut Object = msg_send![item, displayVersionString];
        string(version)
    }
}

fn report(event: UpdateEvent) {
    if let Some(events) = EVENTS.get() {
        let _ = events.try_send(event);
    }
}

unsafe fn nsstring(text: &str) -> *mut Object {
    let text = CString::new(text).expect("static text has no interior nul");
    unsafe { msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()] }
}

unsafe fn string(value: *mut Object) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let utf8: *const c_char = unsafe { msg_send![value, UTF8String] };
    if utf8.is_null() {
        return None;
    }
    Some(
        unsafe { CStr::from_ptr(utf8) }
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::should_start;

    #[test]
    fn only_release_builds_with_a_feed_update_themselves() {
        assert!(should_start(false, true));
        assert!(!should_start(true, true));
        assert!(!should_start(false, false));
        assert!(!should_start(true, false));
    }
}
