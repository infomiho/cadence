//! Reads, writes, and watches the system output volume.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

/// The volume as the operating system reports it, in 0..1, or `None` when the
/// system output has no volume control.
pub(crate) fn output_volume() -> Option<f32> {
    macos::output_volume()
}

/// Sets the system output volume, reporting whether anything changed.
pub(crate) fn set_output_volume(volume: f32) -> bool {
    macos::set_output_volume(volume)
}

/// Where the watcher reports volume changes. Refreshed by every backend
/// worker spawn so events reach the receiver the app is currently reading.
pub(crate) fn set_event_sink(sink: VolumeEventSink) {
    store_sink(sink);
}

/// Starts the background watcher that reports external volume changes.
pub(crate) fn start_watcher() {
    #[cfg(target_os = "macos")]
    spawn_watcher();
}

#[cfg(target_os = "macos")]
mod macos;

#[cfg(not(target_os = "macos"))]
mod macos {
    pub(super) fn output_volume() -> Option<f32> {
        None
    }

    pub(super) fn set_output_volume(_volume: f32) -> bool {
        false
    }
}

type VolumeEventSink = Box<dyn Fn(f32) + Send + Sync>;

static EVENT_SINK: Mutex<Option<VolumeEventSink>> = Mutex::new(None);
static WATCHER_STARTED: AtomicBool = AtomicBool::new(false);

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const VOLUME_EPSILON: f32 = 0.001;

fn store_sink(sink: VolumeEventSink) {
    *EVENT_SINK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sink);
}

fn publish(volume: f32) {
    if let Some(sink) = EVENT_SINK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
    {
        sink(volume);
    }
}

#[cfg(target_os = "macos")]
fn spawn_watcher() {
    if WATCHER_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = thread::Builder::new()
        .name("cadence-volume-watcher".to_owned())
        .spawn(|| {
            let mut last = None;
            loop {
                if let Some(volume) = output_volume()
                    && last.is_none_or(|previous| differs_enough(previous, volume))
                {
                    publish(volume);
                    last = Some(volume);
                }
                thread::sleep(POLL_INTERVAL);
            }
        });
    if let Err(error) = spawned {
        log::warn!("could not start the system volume watcher: {error}");
    }
}

fn differs_enough(previous: f32, current: f32) -> bool {
    (previous - current).abs() > VOLUME_EPSILON
}

#[cfg(test)]
mod tests {
    use super::{differs_enough, output_volume};

    #[test]
    fn sub_epsilon_volume_moves_are_ignored() {
        assert!(!differs_enough(0.5, 0.500_1));
        assert!(differs_enough(0.5, 0.502));
        assert!(differs_enough(0.5, 0.));
    }

    /// Read-only: tests must never change the system volume.
    #[cfg(target_os = "macos")]
    #[test]
    fn system_output_volume_is_within_the_slider_range() {
        if let Some(volume) = output_volume() {
            assert!((0. ..=1.).contains(&volume));
        }
    }
}
