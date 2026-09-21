# Scrubber rework

The player progress bar is click-only, seeks to the wrong place in fullscreen, and
has no keyboard path. This plan covers the diagnosis, the target behaviour and the
implementation order.

## Diagnosis

Everything below is read off the current code, not inferred from using the app.

### Correctness

**1. Fullscreen seeks land ~224 px off.** The mouse handler derives the track's
origin from `window.window_bounds().get_bounds().size.width`
(`src/app/player_bar.rs:446`), while layout sizes the same track from
`window.viewport_size().width` (`src/app/player_bar.rs:253`). On macOS
`window_bounds()` returns `WindowBounds::Fullscreen(fullscreen_restore_bounds)`,
the *pre-fullscreen* frame. A window opened at 1280 and fullscreened on a 1728 pt
display gives the handler an origin of 524 where the track is painted at 748. A
click on the left edge seeks to 1:58 of a 3:00 track and the first two thirds of
the track cannot be reached at all. `volume_for_pointer` reads the same width and
is pinned to maximum under the same conditions.

**2. The handler re-derives geometry instead of measuring it.**
`seek_for_pointer` (`src/app/mod.rs:297`) reconstructs the track's left edge from
seven layout constants and a window width. It is a second, hand-maintained copy of
what the layout engine already computed. Defect 1 is a symptom; the duplication is
the disease. The unit test at `src/app/mod.rs:509` asserts the copy against itself,
so it can never catch the drift.

**3. The compact clamp is only in one of the two copies.** The renderer clamps the
track to `(viewport - 500).max(160.)` (`player_bar.rs:255`); the handler divides by
an unclamped `window_width - 500.` (`mod.rs:300`). Below 660 px wide the seek
overshoots, at exactly 500 px it divides by zero and every click lands on 0:00, and
below 524 px the width goes negative and the scrubber runs backwards. The 720 px
`window_min_size` (`src/app/windows.rs:103`) is the only reason this is unreachable
today.

**4. An unfocused window seeks on the click that focuses it.** gpui hardcodes
`acceptsFirstMouse` to `YES` for the whole window
(`gpui-pre-macos-0.3.4/src/window.rs:3195`), with no per-view override, so clicking
a background Cadence window on the scrubber both activates it and seeks. AppKit's
default for `NSSlider` is the opposite. The event carries the flag
(`MouseDownEvent.first_mouse`, `gpui-pre-0.3.4/src/interactive.rs:162`), so the
handler can ignore that one press.

**5. Seeks are silently swallowed during reconnect.** `Player::seek` only applies
the optimistic position when `send` succeeds, and `send` returns `false` whenever
`restore.is_some()` (`src/app/player.rs:112`). During a reconnect the scrubber is a
dead control with no cursor change, no error and no movement.

**6. A stale position tick can undo a seek.** `BackendEvent::PositionChanged` is
applied whenever the URI matches (`src/app/player.rs:404`), with no sequence or
epoch guard. librespot emits every 250 ms (`src/playback.rs:224`), so a tick
computed before the seek but delivered after it drags the bar backwards for up to
one frame-worth of ticks before `Seeked` corrects it.

### Interaction

**7. There is no drag.** The element has a single `on_mouse_down`
(`player_bar.rs:443`) and no move, up or drag handlers. Click-to-jump is the entire
interaction. This is the thing that makes it unfun.

**8. There is no affordance.** 5 px tall, `cursor_pointer()` and nothing else: no
hover growth, no thumb, no press state. The only feedback that a click registered
is the fill jumping, which is exactly the feedback that defects 1 and 4 remove.

**9. The hit area is the painted strip.** 5 px, versus the volume slider's 24 px
box with a 12 px thumb (`player_bar.rs:493`). Fitts's law says this is the hardest
target in the window, and it is the one that most rewards precision.

**10. No keyboard, no accessibility.** The element has an `ElementId` but no focus
handle, no `tab_index` and no `key_context`. There is no seek action in `actions!`
(`mod.rs:46`) or `bind_keys` (`bootstrap.rs:117`), and no accessible name, role or
value. The control cannot be operated or announced without a mouse.

**11. No widget-level test coverage.** `test_support` has no scrubber interaction
and `Scene` has no progress representation, so none of the above is visible to CI.

### Not worth fixing

The 250 ms position tick quantises the fill to 0.47 px per step on a 340 px track.
Imperceptible. Smooth interpolation is not needed for its own sake, though it comes
along for free if the fill is driven by a spring.

## Target behaviour

Prototype in `design/scrubber/index.html`. Variant D is the proposal; A reproduces
today's bar, B and C isolate the two mainstream alternatives.

### Shape and affordance

| | rest | hover or drag |
|---|---|---|
| track | 5 px | 7 px |
| thumb | hidden | 13 px |
| hit area | 16 px | 16 px |

The hit area stays 16 px at all times: it is invisible, it costs nothing, and it is
the single largest improvement to how the control feels. Measured comparisons:
YouTube pads to 16 px (40 px in touch mode) around a 4 px track that grows to 6 px;
Spotify keeps a fixed 4 px track in a 12 px wrapper and reveals a 12 px thumb.

Growth and thumb reveal animate together over **160 ms** on
`cubic-bezier(0.05, 0, 0, 1)`, between YouTube's modern 200 ms and its legacy
100 ms.

**The active state is `hovered || dragging`, never `hovered` alone.** During a drag
the pointer leaves the element, and in gpui `on_mouse_move` only fires while the
hitbox is hovered, so a hover-only rule collapses the track mid-gesture. YouTube
solves the identical problem on the web by pairing every hover rule with a
`.ytp-drag` class, because pointer capture suppresses hover events there too.

### Gesture

Press, drag, release, with the seek committed **once, on release**. While dragging,
the fill and the elapsed readout follow the pointer and the readout brightens to
mark it as a preview.

Commit-on-release rather than YouTube's commit-on-move because every seek here is a
network round trip to Spotify. Variant C makes the cost audible. Vidstack's
compromise, throttling to 100 ms during the drag, is the fallback if release-only
ever feels unresponsive.

Details that matter:

- **3 px drag threshold.** Under it the gesture is a click, so a twitchy hand cannot
  convert a click into a scrub.
- **Escape cancels** and restores the pre-drag position. Undocumented in both the
  ARIA slider pattern and Radix, and undiscoverable, but free.
- **Release outside the window still commits.** `on_mouse_up_out` alongside
  `on_mouse_up`, as `gpui_base::slider` does.
- **Snapshot the track rect on press** and reuse it for the gesture. Re-measuring
  mid-drag reads a rect the hover growth may have shifted, a documented cause of
  stray seeks.
- **Refuse to seek when the duration is zero**, before metadata arrives. Today
  `fraction * 0` silently seeks to 0:00.

The threshold gates entering live-scrub mode, not whether to seek: a bare click must
still seek. Prior art for the value: Mapbox GL 3 px, Windows `SM_CXDRAG` ~4 px,
Unity 5 px, GTK 8 px.

### Keyboard and accessibility

The APG publishes a Media Seek Slider example that is literally this component:
`role="slider"` with `tabindex="0"`, `aria-valuemin`/`max`/`now` in seconds,
`aria-valuetext`, and decorative shapes marked `aria-hidden`.

- arrows **5 s** (YouTube, video.js and Vidstack all use 5)
- Page keys **30 s**, Home and End to the ends

The APG's PageUp/Down of 15 steps would be 75 s and video.js uses 60 s, both tuned
for hour-long video. 30 s suits a three-minute track.

`aria-valuenow="243"` announces as "243", so `aria-valuetext` carries the whole
message: elapsed of duration. Announce the duration on focus rather than on every
keypress, or each arrow press re-reads the track length.

Note that YouTube's 16 px and Spotify's 12 px hit areas both **fail WCAG 2.2
SC 2.5.8** vertically. 16 px is what this plan proposes, matching YouTube and
failing the same way. Going to 24 px inside a 96 px bar is affordable and would pass;
worth deciding explicitly rather than by default.

Reduced motion: none of the measured players gate the hover grow on it, but a size
change under a stationary pointer is exactly the target class. Drop to the thumb
reveal alone. `gpui_kit::base::spring` already honours `cx.reduce_motion()`.

### Position model

Own the clock instead of mirroring the last sample. Hold an anchor plus a timestamp
and extrapolate, so the fill is smooth regardless of the 250 ms report interval, and
apply reports as drift corrections:

- under 250 ms of disagreement, ignore it
- 250 ms to 2 s, ease onto the reported value over 300 ms
- over 2 s, snap

Tiers from Spotty PR #406, which solved this against the same Spotify engine.

**A confirmation is not enough to trust the next sample.** A report sampled before
the seek can arrive after the backend has confirmed it. The prototype proved this:
an epoch guard alone still let the bar collapse. Hold the requested position until a
report agrees with it within 1 s, or until a 2 s settle window expires and reality
wins, per video.js v10 PR #2744. Measured in the prototype at 600 ms latency, today's
model reads `120 120 0 0 0 0 120 1 1 1 1 1 120 120` after a seek to 2:00, and the
settling model reads `120 120 120 120 121 121 121 122`.

## Implementation

The dependency already ships `gpui_component::Slider`, and it is the wrong choice:
its track height, thumb size and hover ring are hardcoded in its `RenderOnce`, so
grow-on-hover is impossible without fighting it. Its `SliderEvent::Change` /
`Release` split is worth copying, and its unstyled `SliderTrack` / `SliderIndicator`
/ `SliderThumb` parts are worth building on.

1. **Measure instead of deriving.** Capture the track's real `Bounds` with
   `ElementExt::on_prepaint` and compute the fraction from them. Delete
   `seek_for_pointer` and its constants-based twin `volume_for_pointer`, and the test
   that asserts the model against itself. This alone fixes defects 1, 2 and 3, and
   fixes volume in fullscreen for free.
2. **Extract the component.** A `scrubber` module holding geometry, drag state and
   hover state, used by both the progress bar and the volume slider. They are the
   same control with different ranges.
3. **Add the drag.** `.id()` + `.on_drag(payload, ctor)` + `.on_drag_move::<T>()` +
   `on_mouse_up` / `on_mouse_up_out`. The `on_drag` constructor renders `Empty`, as
   `gpui_base::slider` does. `on_drag_move` is the only gpui API that keeps
   delivering moves once the pointer leaves the element.
4. **Animate the grow** with `gpui_kit::base::spring` or `transition`, keyed on
   `hovered || dragging`. `.hover()` snaps, and `with_animation` is one-shot and
   cannot reverse when the pointer leaves mid-grow. Hold the flags in
   `window.use_keyed_state`.
5. **Rework the position model** in `Player`: anchor, timestamp, drift tiers, settle
   window. Surface a swallowed seek during reconnect instead of dropping it silently.
6. **Keyboard and a11y**: focus handle, `tab_index`, a `Seek` action pair bound in
   `bind_keys`, accessible name, role and value. Ignore presses carrying
   `first_mouse`.
7. **Test the element, not the arithmetic.** Add a scrubber interaction to
   `test_support` and a progress representation to `Scene`, then cover press-drag-
   release, the drag threshold, the settle window and fullscreen geometry.

Steps 1 and 5 are the correctness fixes and stand alone. Steps 2 to 4 are the feel.
Step 6 is the accessibility gap. Step 7 is what keeps all of it from regressing.

## Deliberately not doing

- **Precision scrubbing.** YouTube's pull-up and iOS Music's labelled speed tiers
  both work, and iOS's visible tier name is what makes it forgiving. But both are
  built for scrubbing inside long video; a three-minute track at 340 px is already
  0.5 s per pixel. Revisit if long-form audio ever lands.
- **Scroll-wheel seeking.** Apple Music accepts horizontal scroll over the bar and
  mpv and VLC bind it, but the recurring complaint is a wheel changing a value when
  the user meant to scroll. If it is ever added, horizontal only.
- **Shift-drag for fine control.** No media player does it; it is a design-tool
  idiom and would not be discovered.

## As built

Implemented in `src/app/scrubber.rs` and `src/app/playback_clock.rs`. Where the
build departs from the plan above:

- **Hit area is 24 px, not 16.** 16 px matches YouTube and fails WCAG 2.5.8 the
  same way YouTube does. Since the choice was open, the passing one won.
- **No drift easing, and no `display_ms`.** The ease was justified by smooth
  repainting that this app does not do: nothing in `src/app` requests animation
  frames, so the bar repaints only when a backend event notifies, on the 250 ms
  report cadence. A 300 ms ease sampled once reads as the jump it was meant to
  avoid. The clock keeps the anchor, the extrapolation and the settle window,
  which are what make it correct; the easing machinery was removed as complexity
  that could not pay for itself. At 340 px for a three-minute track a report is
  worth half a pixel, so there is nothing to smooth.
- **The drag threshold is gpui's built-in 2 px**, not a hand-rolled 3 px. gpui
  opens a drag only after the pointer moves 2 px, which is inside the 3-5 px range
  the prior art suggests, so the control gets the behaviour for free.
- **Escape abandons the scrub rather than restoring a position.** Since the seek
  commits only on release, a cancelled gesture never moved playback and has
  nothing to undo. Issuing a restoring seek, as the prototype did, would rewind
  playback by the length of the drag.
- **A gesture is stamped with the track it began on** and dropped if that track
  changes, so a press whose release arrives after a track change cannot seek into
  the new track.

- **Dragging to the end ends the track**, as it does everywhere else. An earlier
  revision held seeks a little short of the duration to stop "go to the end"
  becoming "skip". The premise was never confirmed, and the guard made the last
  moment of every track unreachable to buy protection from an outcome the
  listener was asking for anyway.
- **Hover comes from gpui's own hover events**, not from testing the pointer
  against the track's bounds. The hit area is wider than the painted track, and a
  pointer that leaves the window has no position left to test, which left the bar
  stuck open.

Known and accepted: gpui reports hover as false while any button is down, so a
press that does not open a gesture collapses the bar until the pointer moves
again. The `|| dragging()` term covers the path that matters.

Unproven: whether macOS delivers the mouse-up when the button is released outside
the window. The platform sources are not vendored. `on_mouse_up_out` is registered
unconditionally, so a gesture that somehow loses its release is ended by the next
one anywhere in the window.

Still outstanding: the volume slider continues to derive its geometry from layout
constants, so it is still pinned to maximum in fullscreen. It is the same control
with a different range and wants the same treatment.
