const TICK_MS = 250;
const DRIFT_IGNORE_MS = 250;
const DRIFT_EASE_MS = 2000;
const EASE_DURATION_MS = 300;
const SETTLE_TOLERANCE_MS = 1000;
const SETTLE_TIMEOUT_MS = 2000;

const formatDuration = (ms) => {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
};

const clamp = (value, min, max) => Math.min(max, Math.max(min, value));

/*
 * Stands in for librespot: advances in real time, reports position on a 250ms
 * interval, and applies seeks only after a round trip. The reporting delay is
 * what makes a sample stale by the time it lands.
 */
class FakeEngine {
  constructor({ durationMs, latencyMs = 180 }) {
    this.durationMs = durationMs;
    this.latencyMs = latencyMs;
    this.anchorMs = 0;
    this.anchorAt = performance.now();
    this.playing = false;
    this.listeners = new Set();
    setInterval(() => this.report(), TICK_MS);
  }

  on(listener) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  emit(event) {
    for (const listener of this.listeners) listener(event);
  }

  truthMs() {
    const elapsed = this.playing ? performance.now() - this.anchorAt : 0;
    return clamp(this.anchorMs + elapsed, 0, this.durationMs);
  }

  report() {
    const sampled = this.truthMs();
    setTimeout(() => this.emit({ type: "position", positionMs: sampled }), this.latencyMs);
  }

  setPlaying(playing) {
    this.anchorMs = this.truthMs();
    this.anchorAt = performance.now();
    this.playing = playing;
    this.emit({ type: "playing", playing });
  }

  seek(positionMs) {
    setTimeout(() => {
      this.anchorMs = clamp(positionMs, 0, this.durationMs);
      this.anchorAt = performance.now();
      this.emit({ type: "seeked", positionMs: this.anchorMs });
    }, this.latencyMs);
  }
}

/*
 * Turns engine reports into the position the bar draws.
 *
 * "raw" mirrors Cadence today: whatever the last report said, with an
 * optimistic write on seek that a late sample can overwrite.
 *
 * "anchored" owns the clock. It extrapolates from an anchor, ignores reports
 * that belong to a superseded seek, and treats small disagreements as drift to
 * ease away rather than a jump. Tiers follow Spotty PR #406.
 */
class PositionModel {
  constructor(engine, mode) {
    this.engine = engine;
    this.mode = mode;
    this.anchorMs = 0;
    this.anchorAt = performance.now();
    this.playing = false;
    this.settling = null;
    this.ease = null;
    engine.on((event) => this.receive(event));
  }

  receive(event) {
    if (event.type === "playing") {
      this.anchorMs = this.positionMs();
      this.anchorAt = performance.now();
      this.playing = event.playing;
      return;
    }
    if (this.mode === "raw") {
      this.setAnchor(event.positionMs);
      return;
    }
    if (event.type === "seeked") return;
    if (this.settling && !this.hasSettled(event.positionMs)) return;
    this.reconcile(event.positionMs);
  }

  /*
   * A report sampled before the seek can still arrive after the backend has
   * confirmed it, so confirmation alone is not enough to trust the next sample.
   * Hold the requested position until a report agrees with it, or until the
   * settle window expires and reality wins. Follows video.js v10 PR #2744.
   */
  hasSettled(reportedMs) {
    const agrees = Math.abs(reportedMs - this.positionMs()) <= SETTLE_TOLERANCE_MS;
    const expired = performance.now() > this.settling.until;
    if (!agrees && !expired) return false;
    this.settling = null;
    return true;
  }

  reconcile(reportedMs) {
    const drift = reportedMs - this.positionMs();
    if (Math.abs(drift) < DRIFT_IGNORE_MS) return;
    if (Math.abs(drift) > DRIFT_EASE_MS) {
      this.setAnchor(reportedMs);
      return;
    }
    this.ease = { correctionMs: drift, startedAt: performance.now() };
    this.setAnchor(reportedMs);
  }

  setAnchor(positionMs) {
    this.anchorMs = positionMs;
    this.anchorAt = performance.now();
  }

  /* A seek issued elsewhere: move the bar, but do not re-issue it to the engine. */
  adopt(positionMs) {
    this.setAnchor(positionMs);
    this.ease = null;
    if (this.mode === "anchored") {
      this.settling = { until: performance.now() + SETTLE_TIMEOUT_MS };
    }
  }

  seek(positionMs) {
    this.adopt(positionMs);
    this.engine.seek(positionMs);
  }

  positionMs() {
    if (this.mode === "raw") return this.anchorMs;
    const elapsed = this.playing ? performance.now() - this.anchorAt : 0;
    return clamp(this.anchorMs + elapsed, 0, this.engine.durationMs);
  }

  displayMs() {
    const settled = this.positionMs();
    if (!this.ease) return settled;
    const progress = (performance.now() - this.ease.startedAt) / EASE_DURATION_MS;
    if (progress >= 1) {
      this.ease = null;
      return settled;
    }
    const eased = 1 - Math.pow(1 - progress, 5);
    return settled - this.ease.correctionMs * (1 - eased);
  }
}

/*
 * commit: "down"    seek on press, no drag at all (Cadence today)
 *         "move"    seek continuously while dragging (YouTube)
 *         "release" preview while dragging, seek once on release (Spotify)
 */
class Scrubber {
  constructor(root, { model, commit, dragThresholdPx = 0, keyboard = false, escCancel = false, originErrorPx = 0, stepMs = 5000, pageStepMs = 30000 }) {
    this.root = root;
    this.model = model;
    this.commit = commit;
    this.dragThresholdPx = dragThresholdPx;
    this.escCancel = escCancel;
    this.originErrorPx = originErrorPx;
    this.stepMs = stepMs;
    this.pageStepMs = pageStepMs;
    this.dragRect = null;

    this.element = root.querySelector(".scrubber");
    this.track = root.querySelector(".scrub-track");
    this.fill = root.querySelector(".scrub-fill");
    this.thumb = root.querySelector(".scrub-thumb");
    this.hint = root.querySelector(".scrub-hint");
    this.elapsed = root.querySelector(".time.elapsed");
    this.duration = root.querySelector(".time.duration");
    this.readout = root.querySelector(".readout");

    this.hovered = false;
    this.dragging = false;
    this.previewMs = null;
    this.pressOriginX = 0;
    this.passedThreshold = false;
    this.positionBeforeDrag = 0;

    this.element.addEventListener("pointerdown", (event) => this.onPointerDown(event));
    this.element.addEventListener("pointermove", (event) => this.onPointerMove(event));
    this.element.addEventListener("pointerup", (event) => this.onPointerUp(event));
    this.element.addEventListener("pointercancel", () => this.cancelDrag());
    this.element.addEventListener("pointerenter", () => { this.hovered = true; });
    this.element.addEventListener("pointerleave", () => { this.hovered = false; });

    if (keyboard) {
      this.element.tabIndex = 0;
      this.element.setAttribute("role", "slider");
      this.element.setAttribute("aria-label", "Seek");
      this.element.setAttribute("aria-valuemin", "0");
      this.element.setAttribute("aria-valuemax", String(Math.round(model.engine.durationMs / 1000)));
      this.element.addEventListener("keydown", (event) => this.onKeyDown(event));
    }
    if (escCancel) {
      window.addEventListener("keydown", (event) => {
        if (event.key === "Escape" && this.dragging) this.cancelDrag(true);
      });
    }

    const frame = () => {
      this.render();
      requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  }

  /*
   * In fullscreen the handler derives the origin from the pre-fullscreen window
   * width, so it believes the track starts this far left of where it is painted.
   *
   * The rect is snapshotted on press and reused for the gesture: re-measuring
   * mid-drag reads a rect the hover growth may have shifted.
   */
  positionForPointer(clientX) {
    const rect = this.dragRect ?? this.track.getBoundingClientRect();
    const believedLeft = rect.left - this.originErrorPx;
    const fraction = clamp((clientX - believedLeft) / rect.width, 0, 1);
    return fraction * this.model.engine.durationMs;
  }

  onPointerDown(event) {
    if (event.button !== 0) return;
    if (this.model.engine.durationMs <= 0) return;
    this.element.setPointerCapture(event.pointerId);
    this.dragRect = this.track.getBoundingClientRect();
    this.pressOriginX = event.clientX;
    this.passedThreshold = this.dragThresholdPx === 0;
    this.positionBeforeDrag = this.model.positionMs();

    if (this.commit === "down") {
      this.model.seek(this.positionForPointer(event.clientX));
      return;
    }
    this.dragging = true;
    this.previewMs = this.positionForPointer(event.clientX);
    if (this.commit === "move") this.model.seek(this.previewMs);
  }

  onPointerMove(event) {
    if (!this.dragging) return;
    if (!this.passedThreshold) {
      if (Math.abs(event.clientX - this.pressOriginX) < this.dragThresholdPx) return;
      this.passedThreshold = true;
    }
    this.previewMs = this.positionForPointer(event.clientX);
    if (this.commit === "move") this.model.seek(this.previewMs);
  }

  onPointerUp(event) {
    if (!this.dragging) return;
    const target = this.passedThreshold ? this.positionForPointer(event.clientX) : this.positionForPointer(this.pressOriginX);
    this.model.seek(target);
    this.endDrag();
  }

  cancelDrag(restore = false) {
    if (!this.dragging) return;
    if (restore) this.model.seek(this.positionBeforeDrag);
    this.endDrag();
  }

  endDrag() {
    this.dragging = false;
    this.previewMs = null;
    this.dragRect = null;
  }

  onKeyDown(event) {
    const duration = this.model.engine.durationMs;
    const current = this.model.positionMs();
    const jump = {
      ArrowLeft: current - this.stepMs,
      ArrowRight: current + this.stepMs,
      PageDown: current - this.pageStepMs,
      PageUp: current + this.pageStepMs,
      Home: 0,
      End: duration,
    }[event.key];
    if (jump === undefined) return;
    event.preventDefault();
    this.model.seek(clamp(jump, 0, duration));
  }

  render() {
    const duration = this.model.engine.durationMs;
    const shown = this.previewMs ?? this.model.displayMs();
    const fraction = duration === 0 ? 0 : clamp(shown / duration, 0, 1);

    this.element.classList.toggle("is-active", this.hovered || this.dragging);
    this.element.classList.toggle("is-dragging", this.dragging);
    this.fill.style.width = `${fraction * 100}%`;
    if (this.thumb) this.thumb.style.left = `${fraction * 100}%`;
    this.elapsed.textContent = formatDuration(shown);
    this.elapsed.classList.toggle("preview", this.previewMs !== null);
    this.duration.textContent = formatDuration(duration);
    if (this.hint) {
      this.hint.style.left = `${fraction * 100}%`;
      this.hint.textContent = formatDuration(shown);
    }
    if (this.keyboardEnabled !== false && this.element.hasAttribute("role")) {
      this.element.setAttribute("aria-valuenow", String(Math.round(shown / 1000)));
      this.element.setAttribute("aria-valuetext", `${formatDuration(shown)} of ${formatDuration(duration)}`);
    }
    if (this.readout) {
      this.readout.textContent = this.dragging
        ? `dragging → ${formatDuration(shown)}`
        : `position ${formatDuration(this.model.displayMs())}`;
    }
  }
}

export { FakeEngine, PositionModel, Scrubber, formatDuration };
