//! XCLIP: clip block references to a rectangle, polygon or closed polyline (inverted on
//! request); turn the clip off and on, draw it as a polyline, delete it; typed at the command line
//! as AutoCAD's prompts go.

use cadcraft_engine::doc::{EntityKind, Handle};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Input, Session};
use serde_json::json;

/// A 10 × 10 square block "SQ" inserted at (100, 0), and the reference's handle.
fn session() -> (Session, Handle) {
    let mut s = Session::new();
    s.execute("rectang", &json!({ "p1": [0, 0], "p2": [10, 10] })).unwrap();
    s.execute("selectall", &json!({})).unwrap();
    for line in ["block", "SQ", "0,0"] {
        s.cmdline(line).unwrap();
    }
    s.execute("insert", &json!({ "name": "SQ", "at": [100, 0] })).unwrap();
    let h = s
        .doc()
        .unwrap()
        .model
        .iter()
        .filter(|e| matches!(&e.kind, EntityKind::Insert(i) if (i.insert.x - 100.0).abs() < 1e-9))
        .map(|e| e.handle)
        .next()
        .unwrap();
    (s, h)
}

fn clip(s: &Session, h: Handle) -> Option<cadcraft_engine::doc::BlockClip> {
    match &s.doc().unwrap().entity(h).unwrap().kind {
        EntityKind::Insert(i) => i.clip.clone(),
        _ => None,
    }
}

#[test]
fn a_rectangle_in_drawing_coordinates_becomes_a_boundary_in_block_coordinates() {
    let (mut s, h) = session();
    s.execute("xclip", &json!({ "handles": [h.hex()], "rect": [[102, 2], [105, 5]] })).unwrap();
    let c = clip(&s, h).expect("clipped");
    assert!(c.enabled && !c.inverted);
    let b = cadcraft_engine::geom::Bounds2::from_points(c.boundary.iter().copied());
    assert!((b.min - Vec2::new(2.0, 2.0)).len() < 1e-9 && (b.max - Vec2::new(5.0, 5.0)).len() < 1e-9, "{b:?}");
    // Moving the reference moves the clip with it (it stays in block coordinates).
    s.execute("move", &json!({ "handles": [h.hex()], "from": [0, 0], "to": [50, 0] })).unwrap();
    assert_eq!(clip(&s, h).unwrap().boundary, c.boundary);
}

#[test]
fn off_on_polyline_and_delete() {
    let (mut s, h) = session();
    s.execute("xclip", &json!({ "handles": [h.hex()], "points": [[101, 1], [108, 1], [104, 8]], "invert": true })).unwrap();
    assert!(clip(&s, h).unwrap().inverted);
    s.execute("xclip", &json!({ "handles": [h.hex()], "option": "off" })).unwrap();
    assert!(!clip(&s, h).unwrap().enabled);
    s.execute("xclip", &json!({ "handles": [h.hex()], "option": "on" })).unwrap();
    assert!(clip(&s, h).unwrap().enabled);
    let r = s.execute("xclip", &json!({ "handles": [h.hex()], "option": "polyline" })).unwrap();
    assert_eq!(r["handles"].as_array().map(Vec::len), Some(1));
    s.execute("xclip", &json!({ "handles": [h.hex()], "option": "delete" })).unwrap();
    assert!(clip(&s, h).is_none());
    // Undo brings the clip back.
    s.execute("u", &json!({})).unwrap();
    assert!(clip(&s, h).is_some());
}

#[test]
fn a_closed_polyline_is_a_boundary() {
    let (mut s, h) = session();
    let r = s.execute("rectang", &json!({ "p1": [103, 3], "p2": [107, 6] })).unwrap();
    let pl = r["handle"].as_str().unwrap().to_string();
    s.execute("xclip", &json!({ "handles": [h.hex()], "polyline": pl })).unwrap();
    let b = cadcraft_engine::geom::Bounds2::from_points(clip(&s, h).unwrap().boundary.iter().copied());
    assert!((b.min - Vec2::new(3.0, 3.0)).len() < 1e-6 && (b.max - Vec2::new(7.0, 6.0)).len() < 1e-6, "{b:?}");
}

#[test]
fn typed_xclip_asks_as_autocad_does() {
    let (mut s, h) = session();
    s.start("xclip").unwrap();
    s.input(Input::Pick(vec![h])).unwrap();
    s.input(Input::Enter).unwrap(); // done selecting
    s.input(Input::Enter).unwrap(); // New boundary
    s.input(Input::Keyword("Invert clip".into())).unwrap();
    s.input(Input::Keyword("Polygonal".into())).unwrap();
    for p in [[101.0, 1.0], [109.0, 1.0], [109.0, 9.0], [101.0, 9.0]] {
        s.input(Input::Point(Vec2::new(p[0], p[1]))).unwrap();
    }
    s.input(Input::Enter).unwrap();
    assert!(s.running.is_none());
    let c = clip(&s, h).expect("clipped");
    assert!(c.inverted);
    assert_eq!(c.boundary.len(), 4);
    // Rectangular with a selection made first, and Delete.
    s.set_selection(vec![h]);
    s.start("xclip").unwrap();
    s.input(Input::Keyword("Delete".into())).unwrap();
    assert!(clip(&s, h).is_none());
    s.set_selection(vec![h]);
    s.start("xclip").unwrap();
    s.input(Input::Enter).unwrap();
    s.input(Input::Enter).unwrap(); // Rectangular
    s.input(Input::Point(Vec2::new(102.0, 2.0))).unwrap();
    s.input(Input::Point(Vec2::new(104.0, 4.0))).unwrap();
    assert!(clip(&s, h).is_some_and(|c| !c.inverted));
}

#[test]
fn bad_boundaries_are_errors() {
    let (mut s, h) = session();
    assert!(s.execute("xclip", &json!({ "handles": [h.hex()], "points": [[0, 0], [1, 1]] })).is_err());
    assert!(s.execute("xclip", &json!({ "handles": [h.hex()], "points": [[0, 0], [1, 1], [2, 2]] })).is_err(), "no area");
    assert!(s.execute("xclip", &json!({ "handles": [h.hex()] })).is_err(), "no boundary");
    assert!(s.execute("xclip", &json!({ "handles": [] , "rect": [[0, 0], [1, 1]] })).is_err());
}
