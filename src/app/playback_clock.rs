use super::*;
use std::time::Instant;

/// A report this close to the extrapolated position is agreement, not drift.
/// Re-anchoring on it would only add jitter.
const DRIFT_IGNORE_MS: u32 = 250;
/// How close a report must land to the requested position to end settling.
const SETTLE_TOLERANCE_MS: u32 = 1_000;
/// How long to hold a requested position before reality wins.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(2);

/// Playback position as the interface should show it.
///
/// The backend reports every 250ms and applies seeks only after a round trip,
/// so mirroring the last report lets a report sampled before a seek undo it.
/// This owns the clock instead: it holds an anchor, extrapolates from it, and
/// accepts reports only once they agree with what was asked for.
///
/// Extrapolation keeps the position monotone and correct between reports; it
/// does not make the bar move more smoothly, because nothing in the app repaints
/// between backend events. At 340px for a three-minute track a report is worth
/// half a pixel, so there is nothing to smooth.
pub(super) struct PlaybackClock {
    anchor_ms: u32,
    anchor_at: Instant,
    playing: bool,
    /// Set while a requested position is being held against stale reports.
    settling_until: Option<Instant>,
}

impl PlaybackClock {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            anchor_ms: 0,
            anchor_at: now,
            playing: false,
            settling_until: None,
        }
    }

    /// The last confirmed or requested position, without extrapolation. This is
    /// what gets persisted and handed to the system media controls.
    pub(super) fn anchor_ms(&self) -> u32 {
        self.anchor_ms
    }

    /// Where playback has reached, extrapolated from the anchor.
    pub(super) fn position_ms(&self, now: Instant) -> u32 {
        if !self.playing {
            return self.anchor_ms;
        }
        let elapsed = now.saturating_duration_since(self.anchor_at).as_millis();
        self.anchor_ms
            .saturating_add(u32::try_from(elapsed).unwrap_or(u32::MAX))
    }

    pub(super) fn set_playing(&mut self, playing: bool, now: Instant) {
        if self.playing == playing {
            return;
        }
        self.anchor_ms = self.position_ms(now);
        self.anchor_at = now;
        self.playing = playing;
    }

    /// Moves to a requested position and holds it against reports still in flight.
    pub(super) fn seek(&mut self, position_ms: u32, now: Instant) {
        self.anchor_ms = position_ms;
        self.anchor_at = now;
        self.settling_until = Some(now + SETTLE_TIMEOUT);
    }

    /// Moves without settling, for a position the backend has stated outright
    /// rather than one this client asked for.
    pub(super) fn reset(&mut self, position_ms: u32, now: Instant) {
        self.anchor_ms = position_ms;
        self.anchor_at = now;
        self.settling_until = None;
    }

    /// Reconciles a position report from the backend.
    pub(super) fn report(&mut self, reported_ms: u32, now: Instant) {
        if !self.accept_report(reported_ms, now) {
            return;
        }
        if reported_ms.abs_diff(self.position_ms(now)) < DRIFT_IGNORE_MS {
            return;
        }
        self.anchor_ms = reported_ms;
        self.anchor_at = now;
    }

    /// A report sampled before a seek can still arrive after the backend has
    /// confirmed it, so confirmation alone does not make the next report
    /// trustworthy. Hold the requested position until a report agrees with it,
    /// or until the settle window expires and reality wins.
    fn accept_report(&mut self, reported_ms: u32, now: Instant) -> bool {
        let Some(until) = self.settling_until else {
            return true;
        };
        let agrees = reported_ms.abs_diff(self.position_ms(now)) <= SETTLE_TOLERANCE_MS;
        if !agrees && now < until {
            return false;
        }
        self.settling_until = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    #[test]
    fn position_extrapolates_between_reports() {
        let start = Instant::now();
        let mut clock = PlaybackClock::new(start);
        clock.set_playing(true, start);

        assert_eq!(clock.position_ms(at(start, 120)), 120);
        assert_eq!(clock.position_ms(at(start, 1_000)), 1_000);
    }

    #[test]
    fn paused_position_holds() {
        let start = Instant::now();
        let mut clock = PlaybackClock::new(start);
        clock.reset(30_000, start);

        assert_eq!(clock.position_ms(at(start, 5_000)), 30_000);
    }

    #[test]
    fn small_disagreement_is_ignored() {
        let start = Instant::now();
        let mut clock = PlaybackClock::new(start);
        clock.set_playing(true, start);
        clock.report(1_100, at(start, 1_000));

        assert_eq!(clock.position_ms(at(start, 1_000)), 1_000);
    }

    #[test]
    fn stale_report_cannot_undo_a_seek() {
        let start = Instant::now();
        let mut clock = PlaybackClock::new(start);
        clock.set_playing(true, start);
        clock.seek(120_000, at(start, 1_000));

        // Sampled before the seek, delivered after it.
        clock.report(1_100, at(start, 1_200));

        assert_eq!(clock.position_ms(at(start, 1_200)), 120_200);
    }

    #[test]
    fn settling_ends_once_a_report_agrees() {
        let start = Instant::now();
        let mut clock = PlaybackClock::new(start);
        clock.set_playing(true, start);
        clock.seek(120_000, at(start, 1_000));
        clock.report(120_250, at(start, 1_250));

        // Settled, so a later genuine jump is adopted rather than held off.
        clock.report(150_000, at(start, 1_500));
        assert_eq!(clock.position_ms(at(start, 1_500)), 150_000);
    }

    #[test]
    fn settle_window_expires_so_reality_wins() {
        let start = Instant::now();
        let mut clock = PlaybackClock::new(start);
        clock.set_playing(true, start);
        clock.seek(120_000, at(start, 1_000));

        // The seek never took effect; playback is still near the start.
        let expired = at(start, 1_000) + SETTLE_TIMEOUT + Duration::from_millis(1);
        clock.report(4_000, expired);

        assert_eq!(clock.position_ms(expired), 4_000);
    }

    #[test]
    fn position_never_runs_backwards_after_a_seek() {
        let start = Instant::now();
        let mut clock = PlaybackClock::new(start);
        clock.set_playing(true, start);
        clock.seek(120_000, at(start, 500));

        let mut previous = 0;
        for step in 0..20 {
            let now = at(start, 500 + step * 100);
            // Reports sampled before the seek keep arriving for a while.
            if step < 6 {
                clock.report(u32::try_from(500 + step * 100).unwrap(), now);
            }
            let shown = clock.position_ms(now);
            assert!(shown >= previous, "went backwards at step {step}: {shown}");
            previous = shown;
        }
    }
}
