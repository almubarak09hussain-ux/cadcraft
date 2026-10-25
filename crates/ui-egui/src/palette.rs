//! The command palette: every command with a menu place, found by typing part of its name,
//! alias, label or menu path, and started as its menu item starts it ([`CadApp::start`]).
//!
//! On a touch screen it stands in for the menu bar, whose items are too small for a finger: the
//! Commands button beside the command line opens it (softkeys.rs), its rows are finger-sized, and
//! its search field brings up the system keyboard. Elsewhere Ctrl/Cmd+Shift+P (`ui.palette`)
//! opens it. Enter starts the first match, Escape or the close button closes it.

use egui::vec2;

use crate::CadApp;
use crate::theme::Tokens;

/// The most matches listed.
pub const MAX_RESULTS: usize = 60;

/// Finger-sized rows.
const ROW_H: f32 = 44.0;

/// Kept by the app.
#[derive(Clone, Debug, Default)]
pub struct Palette {
    pub open: bool,
    pub query: String,
    /// Focus the search field on the next frame (it was just opened).
    focus: bool,
}

impl Palette {
    /// Open it with an empty search, the field focused.
    pub fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.focus = true;
    }
}

/// A command the palette offers.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: &'static str,
    pub aliases: &'static [&'static str],
    /// Shown name, in the interface language.
    pub label: String,
    /// Menu path, in the interface language: "Draw ▸ Circle ▸ Center, Radius".
    pub path: String,
}

/// Every command with a place in the menus (engine commands, then UI-only ones), each once.
pub fn entries() -> Vec<Entry> {
    let path = |menu: &[&str]| menu.iter().map(|m| crate::i18n::t(m).to_string()).collect::<Vec<_>>().join(" ▸ ");
    let mut out: Vec<Entry> = Vec::new();
    for c in cadcraft_engine::command_specs().iter().filter(|c| !c.menu.is_empty()) {
        out.push(Entry { id: c.id, aliases: c.aliases, label: crate::i18n::t(c.label).to_string(), path: path(c.menu) });
    }
    for (id, label, menu, _) in crate::menus::UI_COMMANDS.iter().filter(|c| !c.2.is_empty()) {
        out.push(Entry { id, aliases: &[], label: crate::i18n::t(label).to_string(), path: path(menu) });
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|e| seen.insert(e.id));
    out
}

/// How well `e` matches the lower-case query `q` (lower is better): the command name or an
/// alias exactly (`alias_of` is the command the alias table gives for `q`), the name's start, a
/// word of the label's start, the label, the menu path.
fn score(e: &Entry, q: &str, alias_of: &str) -> Option<u8> {
    let label = e.label.to_lowercase();
    if e.id == q || e.id == alias_of || e.aliases.iter().any(|a| a.eq_ignore_ascii_case(q)) {
        Some(0)
    } else if e.id.starts_with(q) {
        Some(1)
    } else if label.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(q)) {
        Some(2)
    } else if label.contains(q) {
        Some(3)
    } else if e.path.to_lowercase().contains(q) {
        Some(4)
    } else {
        None
    }
}

/// The entries matching `query` (blank: all of them, by menu path), best first, at most
/// [`MAX_RESULTS`].
pub fn search<'a>(entries: &'a [Entry], query: &str) -> Vec<&'a Entry> {
    let q = query.trim().to_lowercase();
    let mut hits: Vec<(u8, &Entry)> = if q.is_empty() {
        entries.iter().map(|e| (0, e)).collect()
    } else {
        // The command an alias names (L is LINE), from the alias table.
        let alias_of = cadcraft_engine::cmd::resolve_alias(&q);
        entries.iter().filter_map(|e| score(e, &q, &alias_of).map(|s| (s, e))).collect()
    };
    if q.is_empty() {
        hits.sort_by(|a, b| a.1.path.cmp(&b.1.path));
    } else {
        hits.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.label.len().cmp(&b.1.label.len())).then_with(|| a.1.label.cmp(&b.1.label)));
    }
    hits.into_iter().take(MAX_RESULTS).map(|(_, e)| e).collect()
}

/// The palette window, while it is open.
pub fn show(app: &mut CadApp, ctx: &egui::Context) {
    if !app.palette.open {
        return;
    }
    let t = Tokens::get();
    let all = entries();
    let mut open = true;
    let mut chosen: Option<&'static str> = None;
    let screen = ctx.content_rect();
    let width = (screen.width() - 32.0).clamp(240.0, 560.0);
    egui::Window::new(crate::i18n::t("Commands"))
        .id(egui::Id::new("command_palette"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .fixed_size([width, (screen.height() * 0.6).max(200.0)])
        .anchor(egui::Align2::CENTER_TOP, [0.0, 48.0])
        .show(ctx, |ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut app.palette.query)
                    .hint_text(crate::i18n::t("Search commands"))
                    .desired_width(f32::INFINITY)
                    .min_size(vec2(0.0, 32.0)),
            );
            if std::mem::take(&mut app.palette.focus) {
                field.request_focus();
            }
            let hits = search(&all, &app.palette.query);
            if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                chosen = hits.iter().find(|e| enabled(app, e.id)).map(|e| e.id);
            }
            ui.separator();
            if hits.is_empty() {
                ui.label(egui::RichText::new(crate::i18n::t("No command matches.")).color(t.text_dim));
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                for e in hits {
                    let on = enabled(app, e.id);
                    let text = egui::RichText::new(format!("{}   {}", e.label, e.id.to_ascii_uppercase()));
                    let row = ui.add_enabled(on, egui::Button::new(text).min_size(vec2(ui.available_width(), ROW_H)).right_text(e.path.as_str()));
                    if row.clicked() {
                        chosen = Some(e.id);
                    }
                }
            });
        });
    if !open || ui_escape(ctx) {
        app.palette.open = false;
    }
    if let Some(id) = chosen {
        app.palette.open = false;
        app.start(id);
    }
}

/// Whether `id` can run now (UI-only commands always can).
fn enabled(app: &CadApp, id: &str) -> bool {
    cadcraft_engine::cmd::find_command(id).is_none_or(|c| (c.enabled)(&app.session).is_ok())
}

fn ui_escape(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.key_pressed(egui::Key::Escape))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids<'a>(hits: &[&'a Entry]) -> Vec<&'a str> {
        hits.iter().map(|e| e.id).collect()
    }

    #[test]
    fn the_menus_commands_are_offered_once_each() {
        let all = entries();
        assert!(all.iter().any(|e| e.id == "circle"), "an engine command");
        assert!(all.iter().any(|e| e.id == "ui.newtemplate"), "a UI-only command");
        let mut seen = std::collections::HashSet::new();
        assert!(all.iter().all(|e| seen.insert(e.id)), "no command twice");
        assert!(all.iter().all(|e| !e.path.is_empty()));
    }

    #[test]
    fn names_and_aliases_come_first() {
        let all = entries();
        let hits = search(&all, "circle");
        assert_eq!(hits.first().map(|e| e.id), Some("circle"), "{:?}", ids(&hits));
        // L is LINE's alias.
        let hits = search(&all, "L");
        assert_eq!(hits.first().map(|e| e.id), Some("line"), "{:?}", ids(&hits));
    }

    #[test]
    fn labels_and_menu_paths_match_too() {
        let e = |id, label: &str, path: &str| Entry { id, aliases: &[], label: label.into(), path: path.into() };
        let all = vec![
            e("zz1", "Center, Radius", "Draw ▸ Circle ▸ Center, Radius"),
            e("zz2", "Polar Array", "Modify ▸ Array ▸ Polar"),
            e("zz3", "Other", "Tools"),
        ];
        assert_eq!(ids(&search(&all, "rad")), ["zz1"], "a word of the label");
        assert_eq!(ids(&search(&all, "array")), ["zz2"]);
        assert_eq!(ids(&search(&all, "circle")), ["zz1"], "the menu path");
        assert!(search(&all, "nothing like it").is_empty());
        assert_eq!(search(&all, "  ").len(), 3, "blank lists everything");
    }

    #[test]
    fn results_are_capped() {
        let all = entries();
        assert!(all.len() > MAX_RESULTS, "enough commands to cap");
        assert_eq!(search(&all, "").len(), MAX_RESULTS);
    }
}
