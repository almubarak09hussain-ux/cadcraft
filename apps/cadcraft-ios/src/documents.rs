//! The app's Documents folder: where drawings are opened from and saved to on iPad.
//!
//! `Info.plist` sets `UIFileSharingEnabled` and `LSSupportsOpeningDocumentsInPlace`, so the folder
//! shows in the Files app as On My iPad ▸ CADCraft: drawings are copied in (or AirDropped, or
//! saved from Mail) there, and what CADCraft saves is there to share. File ▸ Open… lists the
//! drawings in it ([`Picker`]); File ▸ Save As… saves under the chosen name in it ([`save_path`]).
//! The system document picker (iCloud Drive and other providers) is a follow-up.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Extensions File ▸ Open… lists (compared without case).
const OPENABLE: &[&str] = &["dwg", "dxf", "dwt", "svg", "svgz", "lsp"];

/// Most entries listed: a folder with more is shown truncated, so a huge folder can't stall a frame.
pub const MAX_LISTED: usize = 2000;

/// A drawing in the folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
}

/// The drawings in `dir` that File ▸ Open… offers, sorted by name without case; hidden files and
/// folders are left out. An unreadable folder lists nothing.
pub fn list(dir: &Path) -> Vec<Entry> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Entry> = read
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let openable = !name.starts_with('.')
                && Path::new(&name).extension().and_then(|x| x.to_str()).is_some_and(|x| OPENABLE.iter().any(|o| x.eq_ignore_ascii_case(o)));
            openable.then(|| Entry { path: e.path(), name })
        })
        .take(MAX_LISTED)
        .collect();
    out.sort_by_key(|e| e.name.to_lowercase());
    out
}

/// Where File ▸ Save As… writes `name` (the suggested name, or one typed in `saveas`): its file
/// name in `dir`. Any folder part is dropped, so a save stays in the Documents folder; a name
/// that is empty or only dots becomes `Drawing.dxf`.
pub fn save_path(dir: &Path, name: &str) -> PathBuf {
    let file = name.rsplit(['/', '\\']).next().unwrap_or_default().trim();
    let file = if file.trim_matches('.').is_empty() { "Drawing.dxf" } else { file };
    dir.join(file)
}

/// File ▸ Open…: the list of drawings in the Documents folder, shown as a window until one is
/// chosen or it is closed. The host's `pick_open` service returns no path and asks this to show
/// instead (as the web build does with the browser's picker); the choice is opened next frame.
pub struct Picker {
    dir: PathBuf,
    requested: Rc<Cell<bool>>,
    shown: Option<Vec<Entry>>,
}

impl Picker {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, requested: Rc::new(Cell::new(false)), shown: None }
    }

    /// The `pick_open` service: show the list, and report no path now.
    pub fn service(&self) -> Box<dyn Fn() -> Option<String>> {
        let requested = Rc::clone(&self.requested);
        Box::new(move || {
            requested.set(true);
            None
        })
    }

    /// Show the list if it was asked for; returns the path chosen in it, if one was.
    pub fn ui(&mut self, ctx: &egui::Context) -> Option<String> {
        if self.requested.take() {
            self.shown = Some(list(&self.dir));
        }
        let entries = self.shown.as_ref()?;
        let mut open = true;
        let mut chosen = None;
        let mut refresh = false;
        egui::Window::new(cadcraft_ui_egui::i18n::t("Open Drawing"))
            .id(egui::Id::new("ios_documents_picker"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([420.0, 480.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(cadcraft_ui_egui::i18n::t("Drawings in On My iPad ▸ CADCraft (copy files there with the Files app)."));
                ui.separator();
                if entries.is_empty() {
                    ui.label(cadcraft_ui_egui::i18n::t("No drawings yet."));
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for e in entries {
                        // Full-width rows, tall enough for a finger.
                        let row = egui::Button::new(&e.name).min_size(egui::vec2(ui.available_width(), 44.0));
                        if ui.add(row).clicked() {
                            chosen = Some(e.path.to_string_lossy().into_owned());
                        }
                    }
                });
                ui.separator();
                if ui.add(egui::Button::new(cadcraft_ui_egui::i18n::t("Refresh")).min_size(egui::vec2(96.0, 44.0))).clicked() {
                    refresh = true;
                }
            });
        if refresh {
            self.shown = Some(list(&self.dir));
        }
        if !open || chosen.is_some() {
            self.shown = None;
        }
        chosen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cadcraft-ios-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    #[test]
    fn lists_drawings_sorted_without_case_and_skips_the_rest() {
        let dir = scratch("list");
        for f in ["b.DWG", "A.dxf", "c.dwt", "notes.txt", ".hidden.dxf", "plan.svg", "noext", "tool.lsp"] {
            std::fs::write(dir.join(f), b"0\nEOF\n").ok();
        }
        std::fs::create_dir_all(dir.join("folder.dxf")).ok();
        let names: Vec<_> = list(&dir).into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["A.dxf", "b.DWG", "c.dwt", "plan.svg", "tool.lsp"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_folder_lists_nothing() {
        assert!(list(Path::new("/nonexistent/cadcraft/documents")).is_empty());
    }

    #[test]
    fn saves_stay_in_the_documents_folder() {
        let dir = Path::new("/docs");
        assert_eq!(save_path(dir, "Drawing1.dxf"), dir.join("Drawing1.dxf"));
        assert_eq!(save_path(dir, "../../etc/passwd"), dir.join("passwd"));
        assert_eq!(save_path(dir, "/abs/plan.dwg"), dir.join("plan.dwg"));
        assert_eq!(save_path(dir, "C:\\x\\plan.dwg"), dir.join("plan.dwg"));
        for empty in ["", "  ", "..", "a/", "."] {
            assert_eq!(save_path(dir, empty), dir.join("Drawing.dxf"), "{empty:?}");
        }
    }

    #[test]
    fn the_open_service_asks_for_the_list_and_returns_no_path() {
        let dir = scratch("picker");
        std::fs::write(dir.join("one.dxf"), b"0\nEOF\n").ok();
        let mut picker = Picker::new(dir.clone());
        let pick = picker.service();
        assert_eq!(pick(), None);
        let ctx = egui::Context::default();
        let mut chosen = None;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| chosen = picker.ui(ui.ctx()));
        out.textures_delta.clear();
        assert!(picker.shown.as_ref().is_some_and(|e| e.len() == 1));
        assert_eq!(chosen, None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
