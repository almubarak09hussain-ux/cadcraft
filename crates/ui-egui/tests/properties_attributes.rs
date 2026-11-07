//! Block attribute values in the Properties palette: a selected block reference lists its
//! attributes under Attributes, and a value typed there changes the attribute, as in AutoCAD.

use cadcraft_engine::Session;
use cadcraft_engine::doc::EntityKind;
use cadcraft_ui_egui::{CadApp, Services};
use egui::{Key, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{Queryable, by};
use serde_json::json;

/// A selected reference of block TAG with attribute ROOM = "OFFICE".
fn palettes() -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    let r = app.run("rectang", json!({ "p1": [0, 0], "p2": [10, 5] })).unwrap();
    let a = app.run("attdef", json!({ "tag": "ROOM", "prompt": "Room", "default": "-", "at": [1, 1], "height": 1 })).unwrap();
    app.run("block", json!({ "name": "TAG", "base": [0, 0], "handles": [r["handle"], a["handle"]], "keep": "delete" })).unwrap();
    let ins = app.run("insert", json!({ "name": "TAG", "at": [20, 0], "attribs": { "ROOM": "OFFICE" } })).unwrap();
    app.run("select", json!({ "handles": [ins["handle"]] })).unwrap();
    let mut h = Harness::builder().with_size(vec2(400.0, 1600.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            cadcraft_ui_egui::palettes::right_palettes(app, ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

fn room(app: &CadApp) -> String {
    let d = app.session.doc().unwrap();
    d.model
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Insert(i) => i.attribs.iter().find(|a| a.tag == "ROOM").map(|a| a.text.value.clone()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn an_attribute_value_is_edited_in_properties() {
    let mut h = palettes();
    h.get(by().role(egui::accesskit::Role::TextInput).value("OFFICE")).focus();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    h.get(by().role(egui::accesskit::Role::TextInput).value("OFFICE")).type_text("WORKSHOP");
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(room(h.state()), "WORKSHOP");
    // Undo restores the old value, as for any other edit.
    h.state_mut().run("u", json!({})).unwrap();
    assert_eq!(room(h.state()), "OFFICE");
}
