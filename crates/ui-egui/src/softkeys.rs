//! The on-screen keyboard for touch screens (iPad, tablets in the browser).
//!
//! The command line takes keystrokes without a text field (typing anywhere goes to it), so a
//! touch screen's keyboard never comes up by itself. On a touch screen (or where the host says
//! so: the iPad app), a Keyboard button sits in the drawing area's top-left corner. Tapping it opens the
//! **input strip** at the top of the drawing area, clear of the system keyboard, which covers the
//! bottom of the screen and the command line with it. The strip shows the prompt and what is
//! typed, a row of drafting keys the system keyboard hides behind its symbol pages (`@ < , . -`,
//! Enter, Esc), and the recent commands. While it is open the system keyboard is asked for
//! (egui's IME request; on iPad winit then shows the keyboard). Done closes both.
//!
//! The keys send the same events as a hardware keyboard ([`CadApp::synthetic`]), so they go
//! through [`crate::cmdline::keyboard`] exactly as typing does. Enter is on the strip because
//! the iPad keyboard's return key sends a line feed, which egui-winit drops.

use egui::{Key, Rect, pos2, vec2};

use crate::CadApp;
use crate::theme::Tokens;

/// Kept by the app.
#[derive(Clone, Copy, Debug, Default)]
pub struct SoftKeys {
    /// The input strip and the system keyboard are up.
    pub open: bool,
}

/// What a key sends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Send {
    Text(&'static str),
    Key(Key),
}

/// The drafting keys, in order: relative and polar input, coordinate separators, sign, then
/// Enter and Esc.
pub const KEYS: &[(&str, Send)] = &[
    ("@", Send::Text("@")),
    ("<", Send::Text("<")),
    (",", Send::Text(",")),
    (".", Send::Text(".")),
    ("-", Send::Text("-")),
    ("Enter", Send::Key(Key::Enter)),
    ("Esc", Send::Key(Key::Escape)),
];

/// The most recent commands the strip offers.
pub const MAX_RECENT: usize = 6;

/// Finger-sized keys: Apple's minimum hit target.
const KEY_SIZE: f32 = 44.0;

/// Whether to offer the keyboard: the host is a touch device ([`CadApp::touch_device`]), or a
/// touch has been seen.
pub fn wanted(app: &CadApp, ctx: &egui::Context) -> bool {
    app.touch_device || ctx.input(|i| i.has_touch_screen())
}

/// The commands typed most recently, newest first, each once (upper case, first word only).
pub fn recent_commands(history: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in history.iter().rev() {
        let Some(word) = line.split_whitespace().next() else { continue };
        // Programmatic calls (`cmd {json}`) and transparent prefixes are not offered.
        if word.starts_with(['\'', '(', '{', '-']) {
            continue;
        }
        let word = word.to_ascii_uppercase();
        if !out.contains(&word) {
            out.push(word);
        }
        if out.len() == MAX_RECENT {
            break;
        }
    }
    out
}

/// Queue what `key` sends, as a hardware keyboard would.
pub fn press(app: &mut CadApp, key: Send) {
    match key {
        Send::Text(t) => app.synthetic.push(egui::Event::Text(t.into())),
        Send::Key(key) => {
            for pressed in [true, false] {
                app.synthetic.push(egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE });
            }
        }
    }
}

/// The Commands and Keyboard buttons in `canvas`'s top-left corner, and the input strip at the
/// top of `canvas` while it is open. Call after the command line is drawn.
pub fn show(app: &mut CadApp, ctx: &egui::Context, canvas: Rect) {
    if !wanted(app, ctx) {
        app.softkeys.open = false;
        return;
    }
    let t = Tokens::get();
    if !app.softkeys.open {
        // Commands (the command palette: the menu bar is too small for a finger) and Keyboard, in
        // the drawing area's top-left corner below the viewport label: beside the command line
        // they covered it where the drawing area is narrow (iPad in portrait).
        let size = vec2(96.0, KEY_SIZE);
        let at = pos2(canvas.left() + 8.0, canvas.top() + 30.0);
        egui::Area::new(egui::Id::new("softkeys_toggle")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
            // One above the other: side by side they reached the ViewCube in a narrow drawing area.
            ui.vertical(|ui| {
                if ui.add(egui::Button::new(crate::i18n::t("Commands")).min_size(size)).clicked() {
                    app.palette.open();
                }
                if ui.add(egui::Button::new(crate::i18n::t("Keyboard")).min_size(size)).clicked() {
                    app.softkeys.open = true;
                }
            });
        });
        return;
    }
    let recent = if app.session.prompting() { Vec::new() } else { recent_commands(&app.cmd.history) };
    let prompt = app.session.current_prompt().map(|p| if p.raw { p.display() } else { crate::i18n::t(&p.message).to_string() });
    let width = (canvas.width() - 16.0).clamp(0.0, 720.0);
    let mut pressed = None;
    let mut run = None;
    let mut done = false;
    let strip = egui::Area::new(egui::Id::new("softkeys_strip"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos2(canvas.center().x - width / 2.0, canvas.top() + 8.0))
        .show(ctx, |ui| {
            egui::Frame::new().fill(t.cmd_bg).stroke(egui::Stroke::new(1.0, t.cmd_border)).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
                ui.set_width(width - 16.0);
                // What the command line would show: the prompt and what is typed.
                let line =
                    format!("{}{}|", prompt.map(|p| format!("{p}: ")).unwrap_or_else(|| crate::i18n::t("Command: ").to_string()), app.cmd.buffer);
                ui.label(egui::RichText::new(line).font(crate::theme::body()).color(t.text));
                ui.horizontal_wrapped(|ui| {
                    for (label, key) in KEYS {
                        if ui.add(egui::Button::new(crate::i18n::t(label)).min_size(vec2(KEY_SIZE, KEY_SIZE))).clicked() {
                            pressed = Some(*key);
                        }
                    }
                    if ui.add(egui::Button::new(crate::i18n::t("Done")).min_size(vec2(KEY_SIZE * 1.5, KEY_SIZE))).clicked() {
                        done = true;
                    }
                });
                if !recent.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for name in &recent {
                            if ui.add(egui::Button::new(name).min_size(vec2(KEY_SIZE * 1.5, KEY_SIZE))).clicked() {
                                run = Some(name.clone());
                            }
                        }
                    });
                }
            });
        });
    if let Some(key) = pressed {
        press(app, key);
    }
    if let Some(name) = run {
        app.cmdline(&name);
    }
    if done {
        app.softkeys.open = false;
        return;
    }
    // Ask for the system keyboard, unless a text field has focus (it asks for itself).
    if !ctx.egui_wants_keyboard_input() {
        let rect = strip.response.rect;
        ctx.output_mut(|o| {
            o.ime = Some(egui::output::IMEOutput {
                purpose: egui::IMEPurpose::Normal,
                rect,
                cursor_rect: Rect::from_min_size(rect.left_top() + vec2(8.0, 8.0), vec2(1.0, 16.0)),
                should_interrupt_composition: false,
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_commands_are_newest_first_once_each() {
        let history: Vec<String> =
            ["line 0,0 10,0", "CIRCLE", "Line", "'zoom e", "move", "(command \"line\")", "  ", "trim"].map(String::from).into();
        assert_eq!(recent_commands(&history), ["TRIM", "MOVE", "LINE", "CIRCLE"]);
    }

    #[test]
    fn recent_commands_stop_at_the_limit() {
        let history: Vec<String> = (0..20).map(|i| format!("cmd{i}")).collect();
        let r = recent_commands(&history);
        assert_eq!(r.len(), MAX_RECENT);
        assert_eq!(r.first().map(String::as_str), Some("CMD19"));
    }

    #[test]
    fn keys_send_what_a_keyboard_sends() {
        let mut app = CadApp::new(cadcraft_engine::Session::new(), crate::Services::default());
        press(&mut app, Send::Text("@"));
        press(&mut app, Send::Key(Key::Enter));
        assert!(matches!(app.synthetic.first(), Some(egui::Event::Text(t)) if t == "@"));
        let keys: Vec<bool> = app
            .synthetic
            .iter()
            .filter_map(|e| if let egui::Event::Key { key: Key::Enter, pressed, .. } = e { Some(*pressed) } else { None })
            .collect();
        assert_eq!(keys, [true, false], "a press and a release");
    }
}
