//! The on-screen keyboard (softkeys.rs): the Keyboard button opens the input strip, which asks
//! for the system keyboard, and the strip's keys and recent commands reach the command line as
//! typing does.

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services};
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn harness() -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.run("new", serde_json::json!({})).unwrap();
    app.ui.start_tab = false;
    app.session.settings.dynmode = false;
    app.touch_device = true;
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

/// Hand what the keys queued to egui, as the host's `raw_input_hook` does, and run frames.
fn deliver(h: &mut Harness<'static, CadApp>) {
    let events = std::mem::take(&mut h.state_mut().synthetic);
    for e in events {
        h.event(e);
    }
    h.run_steps(2);
}

fn wants_keyboard(h: &Harness<'static, CadApp>) -> bool {
    h.output().platform_output.ime.is_some()
}

#[test]
fn the_keyboard_button_opens_the_strip_and_asks_for_the_system_keyboard() {
    let mut h = harness();
    assert!(!wants_keyboard(&h), "closed: no keyboard");
    h.get_by_label("Keyboard").click();
    h.run_steps(2);
    assert!(h.state().softkeys.open);
    assert!(wants_keyboard(&h), "open: the system keyboard is asked for");
    h.get_by_label("Done").click();
    h.run_steps(2);
    assert!(!h.state().softkeys.open);
    assert!(!wants_keyboard(&h), "Done hides it again");
}

#[test]
fn strip_keys_type_at_the_command_line() {
    let mut h = harness();
    h.state_mut().cmdline("LINE 0,0");
    h.state_mut().softkeys.open = true;
    h.run_steps(2);
    // "@10<0" typed with the strip's symbols and the system keyboard's digits.
    h.get_by_label("@").click();
    h.run_steps(1);
    deliver(&mut h);
    for c in ["1", "0"] {
        h.event(egui::Event::Text(c.into()));
    }
    h.run_steps(1);
    h.get_by_label("<").click();
    h.run_steps(1);
    deliver(&mut h);
    h.event(egui::Event::Text("0".into()));
    h.run_steps(1);
    assert_eq!(h.state().cmd.buffer, "@10<0");
    h.get_by_label("Enter").click();
    h.run_steps(1);
    deliver(&mut h);
    let line = h.state().session.doc().unwrap().model.handles().len();
    assert_eq!(line, 1, "Enter placed the point: one segment");
    h.get_by_label("Esc").click();
    h.run_steps(1);
    deliver(&mut h);
    assert!(h.state().session.running.is_none(), "Esc ends LINE");
}

#[test]
fn recent_commands_start_again_from_the_strip() {
    let mut h = harness();
    // Typed (and so in the command history), as a user would.
    h.state_mut().cmd.buffer = "CIRCLE 0,0 5".into();
    cadcraft_ui_egui::cmdline::submit(h.state_mut());
    assert!(h.state().session.running.is_none(), "{}", h.state().session.prompt_text());
    h.state_mut().softkeys.open = true;
    h.run_steps(2);
    h.get_by_label("CIRCLE").click();
    h.run_steps(2);
    assert_eq!(h.state().session.running.as_ref().map(|r| r.id.as_str()), Some("circle"));
}
