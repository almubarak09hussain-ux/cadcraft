//! Selection cycling (SELECTIONCYCLING): a click on overlapping objects offers them all and picks
//! the one chosen, with no command running and at "Select objects"; off, a click picks the nearest.

use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Input, Session};
use serde_json::json;

/// Two lines crossing at (5, 5), and their handles.
fn session() -> (Session, String, String) {
    let mut s = Session::new();
    s.viewport_px = (1000.0, 1000.0);
    let a = s.execute("line", &json!({ "points": [[0, 0], [10, 10]] })).unwrap();
    let b = s.execute("line", &json!({ "points": [[0, 10], [10, 0]] })).unwrap();
    let h = |v: &serde_json::Value| v["handles"][0].as_str().or(v["handle"].as_str()).unwrap().to_string();
    (s, h(&a), h(&b))
}

fn handle(s: &Session, hex: &str) -> cadcraft_engine::doc::Handle {
    s.doc().unwrap().model.iter().map(|e| e.handle).find(|h| h.hex() == hex).unwrap()
}

#[test]
fn a_click_on_overlapping_objects_offers_them_all() {
    let (mut s, _a, b) = session();
    s.execute("selectioncycling", &json!({ "on": true })).unwrap();
    s.idle_click(Vec2::new(5.0, 5.0), false).unwrap();
    let c = s.cycling.clone().expect("a list to choose from");
    assert_eq!(c.candidates.len(), 2);
    assert!(s.selection().is_empty(), "nothing picked before a choice");
    let hb = handle(&s, &b);
    s.choose_cycled(Some(hb)).unwrap();
    assert_eq!(s.selection(), vec![hb]);
    assert!(s.cycling.is_none());
    // Dismissing picks nothing.
    s.set_selection(Vec::new());
    s.idle_click(Vec2::new(5.0, 5.0), false).unwrap();
    s.choose_cycled(None).unwrap();
    assert!(s.selection().is_empty());
    // One object under the click: picked at once, no list.
    s.idle_click(Vec2::new(1.0, 1.0), false).unwrap();
    assert!(s.cycling.is_none());
    assert_eq!(s.selection().len(), 1);
}

#[test]
fn at_select_objects_the_chosen_object_is_the_one_picked() {
    let (mut s, a, b) = session();
    s.execute("selectioncycling", &json!({ "on": true })).unwrap();
    s.start("erase").unwrap();
    s.input(Input::Point(Vec2::new(5.0, 5.0))).unwrap();
    let ha = handle(&s, &a);
    s.choose_cycled(Some(ha)).unwrap();
    s.input(Input::Enter).unwrap();
    assert!(s.running.is_none());
    let left: Vec<String> = s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
    assert_eq!(left, vec![b]);
}

#[test]
fn off_a_click_picks_the_nearest_and_bad_choices_are_errors() {
    let (mut s, _a, _b) = session();
    s.idle_click(Vec2::new(5.0, 5.0), false).unwrap();
    assert!(s.cycling.is_none());
    assert_eq!(s.selection().len(), 1);
    s.execute("selectioncycling", &json!({ "on": true })).unwrap();
    s.set_selection(Vec::new());
    s.idle_click(Vec2::new(5.0, 5.0), false).unwrap();
    assert!(s.choose_cycled(Some(cadcraft_engine::doc::Handle(0xFFFF))).is_err());
    assert!(s.selection().is_empty());
}
