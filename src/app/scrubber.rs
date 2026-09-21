use super::*;

/// Payload for the drag that keeps pointer moves arriving once the pointer
/// leaves the track. gpui delivers `on_mouse_move` only while the hitbox is
/// hovered, so a press-drag-release control has to ride the drag machinery.
#[derive(Clone)]
pub(super) struct ScrubberDrag;

/// Rendered under the cursor while dragging. The scrubber draws its own
/// feedback, so this paints nothing.
pub(super) struct NoDragPreview;

impl Render for NoDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::Empty
    }
}

/// One press-drag-release gesture, held for as long as the button is down.
struct Gesture {
    /// The track's bounds as they were when the gesture began. Re-measuring
    /// mid-drag would read a rect the hover growth has already shifted.
    bounds: Bounds<Pixels>,
    /// The track the gesture started on. A gesture that outlives its track
    /// would otherwise land at whatever fraction the pointer sat at.
    track: String,
    duration_ms: u32,
    /// Where the pointer is, shown in place of the playback position until the
    /// gesture commits.
    preview_ms: u32,
}

/// A press-drag-release seek bar.
///
/// Geometry comes from the track's painted bounds rather than from layout
/// constants: a second copy of the layout drifts from the first one, and the
/// drift is invisible to tests that check the copy against itself.
pub(super) struct Scrubber {
    /// The track's bounds, written during prepaint and read when mapping a
    /// pointer position to a time.
    pub(super) track_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Taken from gpui's own hover events rather than measured from the pointer:
    /// the hit area is wider than the painted track, and a pointer that leaves
    /// the window has no position left to measure against.
    pub(super) hovered: bool,
    pub(super) focus_handle: FocusHandle,
    gesture: Option<Gesture>,
}

impl Scrubber {
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            track_bounds: Rc::new(Cell::new(Bounds::default())),
            hovered: false,
            focus_handle: cx.focus_handle(),
            gesture: None,
        }
    }

    pub(super) fn dragging(&self) -> bool {
        self.gesture.is_some()
    }

    /// The position to draw: the drag preview when there is one, and never past
    /// the end of the track, since the clock extrapolates without knowing it.
    pub(super) fn displayed_ms(&self, position_ms: u32, duration_ms: u32) -> u32 {
        self.gesture
            .as_ref()
            .map_or(position_ms, |gesture| gesture.preview_ms)
            .min(duration_ms)
    }

    /// Reports nothing for a track of no width, rather than the 0:00 that
    /// dividing by it would produce: seeking to the start is the worst answer
    /// available here, and silence is recoverable.
    fn position_for(bounds: Bounds<Pixels>, pointer_x: Pixels, duration_ms: u32) -> Option<u32> {
        let width = f32::from(bounds.size.width);
        if width <= 0. {
            return None;
        }
        let offset = f32::from(pointer_x - bounds.origin.x);
        let fraction = (offset / width).clamp(0., 1.);
        Some((fraction * duration_ms as f32) as u32)
    }

    pub(super) fn begin(&mut self, pointer_x: Pixels, track: String, duration_ms: u32) {
        let bounds = self.track_bounds.get();
        let Some(preview_ms) = Self::position_for(bounds, pointer_x, duration_ms) else {
            return;
        };
        self.gesture = Some(Gesture {
            bounds,
            track,
            duration_ms,
            preview_ms,
        });
    }

    pub(super) fn drag_to(&mut self, pointer_x: Pixels) {
        let Some(gesture) = self.gesture.as_mut() else {
            return;
        };
        if let Some(preview_ms) = Self::position_for(gesture.bounds, pointer_x, gesture.duration_ms)
        {
            gesture.preview_ms = preview_ms;
        }
    }

    /// Ends the gesture, reporting the position to commit. Reports nothing when
    /// the track it began on is no longer the one playing.
    pub(super) fn release(&mut self, track: Option<&str>) -> Option<u32> {
        let gesture = self.gesture.take()?;
        (Some(gesture.track.as_str()) == track).then_some(gesture.preview_ms)
    }

    /// Abandons the gesture. No seek was ever sent, so there is nothing to undo
    /// and nothing to restore; the preview simply stops standing in.
    pub(super) fn cancel(&mut self) -> bool {
        self.gesture.take().is_some()
    }

    /// The track an in-flight gesture belongs to.
    pub(super) fn gesture_track(&self) -> Option<&str> {
        self.gesture.as_ref().map(|gesture| gesture.track.as_str())
    }
}
