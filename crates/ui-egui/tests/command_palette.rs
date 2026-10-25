//! The command palette (palette.rs): the Commands button on a touch device and Ctrl/Cmd+Shift+P
//! open it, typing narrows it, and Enter or a tapped row starts the command as its menu does.

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services};
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn harness(touch: bool) -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.run("new", serde_json::json!({})).unwrap();
    app.ui.start_tab = false;
    app.touch_device = touch;
    let mut h = Harness::builder().with_size(vec2(1366.0, 1024.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(3);
    h
}

fn running(h: &Harness<'static, CadApp>) -> Option<String> {
    h.state().session.running.as_ref().map(|r| r.id.clone())
}

#[test]
fn the_commands_button_opens_it_and_enter_starts_the_best_match() {
    let mut h = harness(true);
    h.get_by_label("Commands").click();
    h.run_steps(2);
    assert!(h.state().palette.open);
    // The search field has focus: typing goes to it, not the command line.
    h.event(egui::Event::Text("circle".into()));
    h.run_steps(1);
    assert_eq!(h.state().palette.query, "circle");
    assert!(h.state().cmd.buffer.is_empty(), "typing went to the palette");
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert!(!h.state().palette.open, "choosing closes it");
    assert_eq!(running(&h).as_deref(), Some("circle"));
}

#[test]
fn a_tapped_row_starts_its_command() {
    let mut h = harness(true);
    h.state_mut().palette.open();
    h.run_steps(2);
    h.event(egui::Event::Text("rectang".into()));
    h.run_steps(2);
    h.get_by_label_contains("RECTANG").click();
    h.run_steps(2);
    assert_eq!(running(&h).as_deref(), Some("rectang"));
}

#[test]
fn the_shortcut_opens_it_on_a_desktop_and_escape_closes_it() {
    let mut h = harness(false);
    assert!(h.query_by_label("Commands").is_none(), "no touch buttons on a desktop");
    h.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::P);
    h.run_steps(2);
    assert!(h.state().palette.open);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(!h.state().palette.open);
}
