//! Touch screens on the drawing area (iPad, touch laptops, tablets in the browser).
//!
//! egui turns the first finger into the mouse pointer, which is what drawing with one finger or
//! the Apple Pencil needs: a tap places a point or picks an object. The rest is decided here:
//!
//! - Two or more fingers pan (the drawing follows them) and pinch to zoom about their centre.
//!   The first finger is also the pointer, so nothing it does during the gesture counts: no point,
//!   no selection window, until every finger is lifted.
//! - A long press is a right-click (Enter while a command runs, else the shortcut menu); the
//!   finger coming up after it places nothing.
//! - When a finger lifts, egui forgets the pointer in the same frame (`PointerGone`); the tap's
//!   position is still known as the interaction position, and the canvas uses it.
//! - While the last input was a finger, pick boxes, grips and snap targets are
//!   [`FINGER_SCALE`] times their size ([`cadcraft_engine::Session::set_pointer_scale`]); moving a
//!   mouse or trackpad pointer brings them back.

use egui::{Pos2, Vec2};

/// The largest zoom one frame of a pinch applies (and the inverse, the smallest): a jump in the
/// reported finger distance can't fling the view.
const MAX_FRAME_ZOOM: f32 = 4.0;

/// How much larger pick boxes, grips and snap targets are for a finger: PICKBOX 3 picks within
/// about 11 points of the touch, GRIPSIZE 5 grips are 25 points across.
pub const FINGER_SCALE: f64 = 2.5;

/// Kept across frames by the canvas.
#[derive(Clone, Copy, Debug, Default)]
pub struct Touch {
    /// A gesture (several fingers, or a long press) started and a finger is still down: the
    /// pointer's clicks and drags don't count.
    claimed: bool,
    /// The last pointer input was a finger (not a mouse, trackpad or pen hovering).
    finger: bool,
}

/// What touch input means for the canvas this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TouchFrame {
    /// Two-finger pan and pinch: (movement of the fingers' centre in points, zoom factor, centre).
    pub gesture: Option<(Vec2, f32, Pos2)>,
    /// The pointer's primary clicks, drags and presses are part of a gesture and are ignored.
    pub suppress_primary: bool,
    /// A long press: treat as a right-click.
    pub secondary: bool,
    /// Where a finger that just lifted was, when egui no longer has a hover position.
    pub lifted_at: Option<Pos2>,
}

impl Touch {
    /// Whether the last pointer input was a finger.
    pub fn finger(&self) -> bool {
        self.finger
    }

    /// Read this frame's touch input. `long_touched` is the canvas response's `long_touched()`.
    pub fn update(&mut self, i: &egui::InputState, long_touched: bool) -> TouchFrame {
        let touch_events = i.raw.events.iter().any(|e| matches!(e, egui::Event::Touch { .. }));
        // egui-winit moves the pointer with the first finger in the same frame as its touch, so
        // a pointer move without one is a mouse or trackpad.
        if touch_events {
            self.finger = true;
        } else if i.raw.events.iter().any(|e| matches!(e, egui::Event::PointerMoved(_) | egui::Event::MouseWheel { .. })) {
            self.finger = false;
        }
        let gesture = i.multi_touch().filter(|m| m.num_touches >= 2).map(|m| {
            let zoom = if m.zoom_delta.is_finite() && m.zoom_delta > 0.0 { m.zoom_delta.clamp(1.0 / MAX_FRAME_ZOOM, MAX_FRAME_ZOOM) } else { 1.0 };
            let pan = if m.translation_delta.is_finite() { m.translation_delta } else { Vec2::ZERO };
            (pan, zoom, m.center_pos)
        });
        if gesture.is_some() || long_touched {
            self.claimed = true;
        }
        let out = TouchFrame {
            gesture,
            suppress_primary: self.claimed,
            secondary: long_touched,
            lifted_at: if touch_events && i.pointer.hover_pos().is_none() { i.pointer.interact_pos() } else { None },
        };
        // The frame the last finger lifts is still the gesture's; the next touch starts afresh.
        if !i.any_touches() {
            self.claimed = false;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, RawInput, TouchDeviceId, TouchId, TouchPhase, pos2};

    fn touch(id: u64, phase: TouchPhase, pos: Pos2) -> Event {
        Event::Touch { device_id: TouchDeviceId(0), id: TouchId(id), phase, pos, force: None }
    }

    /// Run a frame with `events` (what egui-winit sends) and return what [`Touch`] reads.
    fn frame(ctx: &egui::Context, t: &mut Touch, events: Vec<Event>) -> TouchFrame {
        let raw = RawInput { events, screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))), ..Default::default() };
        let mut got = TouchFrame::default();
        let mut out = ctx.run_ui(raw, |ui| got = ui.input(|i| t.update(i, false)));
        out.textures_delta.clear();
        got
    }

    /// egui-winit's events for a finger: a touch, plus the pointer for the first finger.
    fn down(id: u64, p: Pos2, pointer: bool) -> Vec<Event> {
        let mut v = vec![touch(id, TouchPhase::Start, p)];
        if pointer {
            v.push(Event::PointerMoved(p));
            v.push(Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
        }
        v
    }

    fn up(id: u64, p: Pos2, pointer: bool) -> Vec<Event> {
        let mut v = vec![touch(id, TouchPhase::End, p)];
        if pointer {
            v.push(Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
            v.push(Event::PointerGone);
        }
        v
    }

    #[test]
    fn a_tap_reports_where_the_finger_lifted() {
        let ctx = egui::Context::default();
        let mut t = Touch::default();
        let f = frame(&ctx, &mut t, down(1, pos2(100.0, 120.0), true));
        assert!(!f.suppress_primary && f.gesture.is_none());
        let f = frame(&ctx, &mut t, up(1, pos2(100.0, 120.0), true));
        assert_eq!(f.lifted_at, Some(pos2(100.0, 120.0)));
        assert!(!f.suppress_primary);
    }

    #[test]
    fn two_fingers_pan_and_pinch_and_their_release_places_nothing() {
        let ctx = egui::Context::default();
        let mut t = Touch::default();
        frame(&ctx, &mut t, down(1, pos2(100.0, 100.0), true));
        let f = frame(&ctx, &mut t, down(2, pos2(200.0, 100.0), false));
        assert!(f.suppress_primary, "the second finger turns the touch into a gesture");
        // Both fingers move right by 10 and apart: a pan and a zoom in.
        let f = frame(
            &ctx,
            &mut t,
            vec![
                touch(1, TouchPhase::Move, pos2(100.0, 100.0)),
                Event::PointerMoved(pos2(100.0, 100.0)),
                touch(2, TouchPhase::Move, pos2(240.0, 100.0)),
            ],
        );
        let (pan, zoom, centre) = f.gesture.expect("two fingers are a gesture");
        assert!(pan.x > 0.0 && zoom > 1.0, "pan {pan:?} zoom {zoom}");
        assert!((centre.x - 170.0).abs() < 1e-3, "centre {centre:?}");
        // One finger lifts: still the gesture's, so the first finger's drag counts for nothing.
        let f = frame(&ctx, &mut t, up(2, pos2(240.0, 100.0), false));
        assert!(f.suppress_primary && f.gesture.is_none());
        // The last finger (the pointer) lifts: that release is suppressed too.
        let f = frame(&ctx, &mut t, up(1, pos2(100.0, 100.0), true));
        assert!(f.suppress_primary);
        // A new tap afterwards counts again.
        let f = frame(&ctx, &mut t, down(1, pos2(50.0, 50.0), true));
        assert!(!f.suppress_primary);
    }

    #[test]
    fn a_long_press_is_a_right_click_and_its_release_places_nothing() {
        let ctx = egui::Context::default();
        let mut t = Touch::default();
        frame(&ctx, &mut t, down(1, pos2(10.0, 10.0), true));
        let mut got = TouchFrame::default();
        let mut out = ctx.run_ui(RawInput::default(), |ui| got = ui.input(|i| t.update(i, true)));
        out.textures_delta.clear();
        assert!(got.secondary && got.suppress_primary);
        let f = frame(&ctx, &mut t, up(1, pos2(10.0, 10.0), true));
        assert!(f.suppress_primary && !f.secondary);
    }

    #[test]
    fn a_finger_until_the_mouse_moves() {
        let ctx = egui::Context::default();
        let mut t = Touch::default();
        assert!(!t.finger());
        frame(&ctx, &mut t, down(1, pos2(10.0, 10.0), true));
        assert!(t.finger());
        frame(&ctx, &mut t, up(1, pos2(10.0, 10.0), true));
        frame(&ctx, &mut t, Vec::new());
        assert!(t.finger(), "still a finger after it lifts");
        frame(&ctx, &mut t, vec![Event::PointerMoved(pos2(20.0, 20.0))]);
        assert!(!t.finger(), "a mouse move is not a finger");
    }

    #[test]
    fn mouse_input_is_left_alone() {
        let ctx = egui::Context::default();
        let mut t = Touch::default();
        let p = pos2(30.0, 40.0);
        let f = frame(&ctx, &mut t, vec![Event::PointerMoved(p)]);
        assert_eq!(f, TouchFrame::default());
        let f = frame(&ctx, &mut t, vec![Event::PointerGone]);
        assert_eq!(f.lifted_at, None, "a mouse leaving the window is not a lifted finger");
    }
}
