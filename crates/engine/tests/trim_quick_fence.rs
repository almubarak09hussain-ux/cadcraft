//! TRIM and EXTEND in Quick mode, as in AutoCAD: a pick in empty space starts a two-point
//! crossing fence that trims what it crosses; the prompt takes a press-and-drag freehand path
//! (entered as a fence); Standard mode still opens a crossing window there.

use cadcraft_engine::doc::EntityKind;
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Input, Session};
use serde_json::json;

/// A vertical cutting line x = 50 and three horizontal lines y = 10, 20, 30 from x 0 to 100.
fn session() -> Session {
    let mut s = Session::new();
    s.viewport_px = (1000.0, 1000.0);
    s.execute("line", &json!({ "points": [[50, -10], [50, 50]] })).unwrap();
    for y in [10, 20, 30] {
        s.execute("line", &json!({ "points": [[0, y], [100, y]] })).unwrap();
    }
    s.execute("zoom", &json!({ "extents": true })).unwrap();
    s
}

/// The horizontal lines' x ranges, bottom to top.
fn spans(s: &Session) -> Vec<(f64, f64)> {
    let mut v: Vec<(f64, f64, f64)> = s
        .doc()
        .unwrap()
        .model
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::Line(l) if (l.a.y - l.b.y).abs() < 1e-9 => Some((l.a.y, l.a.x.min(l.b.x), l.a.x.max(l.b.x))),
            _ => None,
        })
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v.into_iter().map(|(_, a, b)| (a, b)).collect()
}

#[test]
fn quick_trim_starts_a_fence_in_empty_space() {
    let mut s = session();
    s.start("trim").unwrap();
    assert!(s.current_prompt().is_some_and(|p| p.freehand), "Quick mode takes a freehand path");
    // Two empty points right of the cutting edge, across y = 10 and 20.
    s.input(Input::Point(Vec2::new(75.0, 5.0))).unwrap();
    assert_eq!(s.current_prompt().unwrap().message, "Specify second fence point");
    s.input(Input::Point(Vec2::new(75.0, 25.0))).unwrap();
    let v = spans(&s);
    assert!((v[0].1 - 50.0).abs() < 1e-6 && (v[1].1 - 50.0).abs() < 1e-6, "{v:?}");
    assert!((v[2].1 - 100.0).abs() < 1e-6, "the line the fence missed stays: {v:?}");
    s.input(Input::Enter).unwrap();
    assert!(s.running.is_none());
}

#[test]
fn a_freehand_path_trims_every_line_it_crosses() {
    let mut s = session();
    s.start("trim").unwrap();
    // What the drawing area sends for a drag: Fence, the path's points, Enter.
    s.input(Input::Keyword("Fence".into())).unwrap();
    for k in 0..=40 {
        let t = f64::from(k) / 40.0;
        s.input(Input::Point(Vec2::new(20.0 + 5.0 * (t * 12.0).sin(), 5.0 + 30.0 * t))).unwrap();
    }
    s.input(Input::Enter).unwrap();
    for (a, b) in spans(&s) {
        assert!((a - 50.0).abs() < 1e-6 && (b - 100.0).abs() < 1e-6, "left of the edge trimmed: {a}..{b}");
    }
    // Still at "Select object to trim" (with Undo putting it back).
    s.input(Input::Keyword("Undo".into())).unwrap();
    assert!(spans(&s).iter().all(|(a, _)| a.abs() < 1e-6));
}

#[test]
fn extend_quick_fence_and_standard_crossing_window() {
    let mut s = Session::new();
    s.viewport_px = (1000.0, 1000.0);
    s.execute("line", &json!({ "points": [[100, -10], [100, 50]] })).unwrap();
    s.execute("line", &json!({ "points": [[0, 10], [60, 10]] })).unwrap();
    s.execute("zoom", &json!({ "extents": true })).unwrap();
    s.start("extend").unwrap();
    assert!(s.current_prompt().is_some_and(|p| p.freehand));
    s.input(Input::Point(Vec2::new(55.0, 0.0))).unwrap();
    s.input(Input::Point(Vec2::new(55.0, 20.0))).unwrap();
    assert!(spans(&s).iter().any(|(_, b)| (b - 100.0).abs() < 1e-6), "{:?}", spans(&s));
    s.input(Input::Enter).unwrap();
    // Standard mode: no freehand path, an empty pick opens a crossing window.
    s.execute("setvar", &json!({ "name": "TRIMEXTENDMODE", "value": 0 })).unwrap();
    s.start("trim").unwrap();
    s.input(Input::Enter).unwrap(); // cutting edges: all
    assert!(s.current_prompt().is_some_and(|p| !p.freehand));
    s.input(Input::Point(Vec2::new(150.0, 40.0))).unwrap();
    assert_eq!(s.current_prompt().unwrap().message, "Specify opposite corner");
}
