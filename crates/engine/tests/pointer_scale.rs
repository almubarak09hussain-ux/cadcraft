//! The pointing device's pick scale (`Session::set_pointer_scale`): a finger picks from farther
//! away than a mouse, PICKBOX itself never changes, and hostile scales are clamped.

use cadcraft_engine::{Session, View};
use cadcraft_geom::Vec2;
use serde_json::json;

/// A session with one line from (0,0) to (100,0), 0.1 units per pixel.
fn session() -> Session {
    let mut s = Session::new();
    s.execute("line", &json!({"points": [[0, 0], [100, 0]]})).unwrap();
    s.viewport_px = (1000.0, 1000.0);
    s.state_mut().unwrap().set_view(View { center: Vec2::new(50.0, 0.0), height: 100.0 });
    s.set_selection(Vec::new());
    s
}

#[test]
fn a_finger_picks_from_farther_away_than_a_mouse() {
    // PICKBOX 3: a mouse picks within 0.45 units here, a finger (2.5×) within 1.125.
    let near = Vec2::new(50.0, 0.8);
    let mut mouse = session();
    let pickbox = mouse.settings.pickbox;
    mouse.idle_click(near, false).unwrap();
    assert!(mouse.selection().is_empty(), "0.8 units off is outside a mouse's pick box");

    let mut finger = session();
    finger.set_pointer_scale(2.5);
    finger.idle_click(near, false).unwrap();
    assert_eq!(finger.selection().len(), 1, "a finger picks the line");
    assert_eq!(finger.settings.pickbox, pickbox, "PICKBOX keeps its value");
    assert!((finger.pick_aperture() - mouse.pick_aperture() * 2.5).abs() < 1e-12);
}

#[test]
fn hostile_pointer_scales_are_clamped() {
    let mut s = session();
    for (given, kept) in [
        (f64::NAN, 1.0),
        (f64::INFINITY, 1.0),
        (f64::NEG_INFINITY, 1.0),
        (-3.0, 1.0),
        (0.0, 1.0),
        (0.5, 1.0),
        (2.5, 2.5),
        (1e300, Session::MAX_POINTER_SCALE),
    ] {
        s.set_pointer_scale(given);
        assert_eq!(s.pointer_scale(), kept, "{given}");
        assert!(s.pick_aperture().is_finite());
    }
}
