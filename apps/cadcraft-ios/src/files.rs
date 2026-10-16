//! Where the iPad app keeps drawings and settings, and the in-app Open browser.
//!
//! Drawings live in the app's Documents folder. With `UIFileSharingEnabled` and
//! `LSSupportsOpeningDocumentsInPlace` in Info.plist the Files app shows it as
//! On My iPad › CADCraft, so DWG and DXF files are copied in and out there. Until the system
//! document picker is wired up, File › Open lists that folder ([`Browser`]).

use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use cadcraft_ui_egui::i18n;

/// File types the browser lists (what OPEN reads).
const DRAWING_EXTENSIONS: &[&str] = &["dwg", "dxf", "dwt", "svg", "svgz"];

/// Most files listed; a folder holding more shows the first ones by name.
const MAX_LISTED: usize = 2000;

/// Directory entries looked at while listing (a folder of other files is not read without end).
const MAX_SCANNED: usize = 20_000;

/// Name a save falls back to when the drawing's name has no usable file name.
const FALLBACK_NAME: &str = "Drawing.dxf";

/// Where drawings live: `CADCRAFT_DOCUMENTS` if set, else the app's Documents folder on iOS and
/// `Documents/CADCraft iPad` elsewhere.
pub fn documents_dir() -> Option<PathBuf> {
    documents_dir_from(|k| std::env::var_os(k))
}

/// Settings, eframe state and autosave files: `CADCRAFT_CONFIG_DIR` if set, else
/// `Library/Application Support/CADCraft` in the app's container on iOS and a folder separate
/// from the desktop app's elsewhere.
pub fn support_dir() -> Option<PathBuf> {
    support_dir_from(|k| std::env::var_os(k))
}

/// [`documents_dir`] with the environment passed in. An empty variable counts as unset, so a
/// blank value never means the working directory.
fn documents_dir_from(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let set = |k: &str| var(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(dir) = set("CADCRAFT_DOCUMENTS") {
        return Some(dir);
    }
    if cfg!(target_os = "ios") {
        set("HOME").map(|h| h.join("Documents"))
    } else if cfg!(windows) {
        set("USERPROFILE").map(|h| h.join("Documents").join("CADCraft iPad"))
    } else {
        set("HOME").map(|h| h.join("Documents").join("CADCraft iPad"))
    }
}

/// [`support_dir`] with the environment passed in.
fn support_dir_from(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let set = |k: &str| var(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(dir) = set("CADCRAFT_CONFIG_DIR") {
        return Some(dir);
    }
    if cfg!(target_os = "ios") {
        set("HOME").map(|h| h.join("Library").join("Application Support").join("CADCraft"))
    } else if cfg!(windows) {
        set("APPDATA").map(|a| a.join("CADCraft iPad"))
    } else {
        set("HOME").map(|h| h.join(".config").join("cadcraft-ipad"))
    }
}

/// Whether OPEN reads `path` (by extension, any case).
pub fn is_drawing(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| DRAWING_EXTENSIONS.iter().any(|d| e.eq_ignore_ascii_case(d)))
}

/// Where Save puts a drawing called `name`: always inside `dir`, under the name's last
/// component, so a name holding a path (or `..`) never writes elsewhere.
pub fn save_path(dir: &Path, name: &str) -> PathBuf {
    // Both separators: a name typed on an iPad may use either.
    let last = name.rsplit(['/', '\\']).next().unwrap_or_default().trim();
    let file = if last.is_empty() || last == "." || last == ".." { FALLBACK_NAME } else { last };
    dir.join(file)
}

/// A drawing in the Documents folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
}

/// The drawings directly inside `dir`, by name (case-insensitive), at most [`MAX_LISTED`].
pub fn list(dir: &Path) -> Result<Vec<Entry>, String> {
    let read = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut entries: Vec<Entry> = read
        .take(MAX_SCANNED)
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()) && is_drawing(&e.path()))
        .map(|e| Entry { name: e.file_name().to_string_lossy().into_owned(), path: e.path(), bytes: e.metadata().map(|m| m.len()).unwrap_or(0) })
        .collect();
    entries.sort_by_cached_key(|e| e.name.to_lowercase());
    entries.truncate(MAX_LISTED);
    Ok(entries)
}

/// A size for people: "820 bytes", "14.2 KB", "3.1 MB".
fn human_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    // Precision loss above 2^53 bytes is irrelevant for a size label.
    #[allow(clippy::cast_precision_loss)]
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} bytes")
    } else if b < KB * KB {
        format!("{:.1} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else {
        format!("{:.1} GB", b / (KB * KB * KB))
    }
}

/// The in-app Open window over the Documents folder. Cloned into the Open picker service, which
/// only asks for the window; [`Browser::show`] draws it and reports the chosen file.
#[derive(Clone)]
pub struct Browser {
    dir: Option<PathBuf>,
    open: Rc<Cell<bool>>,
    /// The folder's drawings, read when the window opens or Refresh is pressed.
    listing: Rc<RefCell<Option<Result<Vec<Entry>, String>>>>,
}

impl Browser {
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self { dir, open: Rc::new(Cell::new(false)), listing: Rc::new(RefCell::new(None)) }
    }

    /// Open the window (File › Open); the folder is read again.
    pub fn request(&self) {
        self.listing.replace(None);
        self.open.set(true);
    }

    fn read(&self) -> Result<Vec<Entry>, String> {
        match &self.dir {
            Some(dir) => list(dir),
            None => Err(i18n::t("This device has no Documents folder for CADCraft.").to_string()),
        }
    }

    /// Draw the window while it is open; the path of the drawing picked this frame, if any.
    pub fn show(&self, ctx: &egui::Context) -> Option<String> {
        if !self.open.get() {
            return None;
        }
        let listing = self.listing.borrow_mut().get_or_insert_with(|| self.read()).clone();
        let mut chosen = None;
        let mut refresh = false;
        let mut cancel = false;
        let mut keep_open = true;
        egui::Window::new(i18n::t("Open Drawing"))
            .collapsible(false)
            .default_size([520.0, 420.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut keep_open)
            .show(ctx, |ui| {
                ui.label(i18n::t("Drawings in On My iPad › CADCraft (copy DWG and DXF files there with the Files app):"));
                ui.add_space(6.0);
                match &listing {
                    Err(e) => {
                        ui.label(e);
                    }
                    Ok(entries) if entries.is_empty() => {
                        ui.label(i18n::t("No drawings yet."));
                    }
                    Ok(entries) => {
                        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                            for e in entries {
                                let row =
                                    ui.add_sized([ui.available_width(), 28.0], egui::Button::new(format!("{}    {}", e.name, human_size(e.bytes))));
                                if row.clicked() {
                                    chosen = Some(e.path.to_string_lossy().into_owned());
                                }
                            }
                        });
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    refresh = ui.button(i18n::t("Refresh")).clicked();
                    cancel = ui.button(i18n::t("Cancel")).clicked();
                });
            });
        if refresh {
            self.listing.replace(None);
        }
        if chosen.is_some() || cancel || !keep_open {
            self.open.set(false);
        }
        chosen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |k| vars.iter().find(|(n, _)| *n == k).map(|(_, v)| OsString::from(v))
    }

    #[test]
    fn overrides_win_and_empty_values_are_unset() {
        let vars = [("CADCRAFT_DOCUMENTS", "/d"), ("CADCRAFT_CONFIG_DIR", "/c"), ("HOME", "/h"), ("USERPROFILE", "/u"), ("APPDATA", "/a")];
        assert_eq!(documents_dir_from(env(&vars)), Some(PathBuf::from("/d")));
        assert_eq!(support_dir_from(env(&vars)), Some(PathBuf::from("/c")));
        let empty = [("CADCRAFT_DOCUMENTS", ""), ("CADCRAFT_CONFIG_DIR", ""), ("HOME", ""), ("USERPROFILE", ""), ("APPDATA", "")];
        assert_eq!(documents_dir_from(env(&empty)), None, "no usable variable means no folder, not the working directory");
        assert_eq!(support_dir_from(env(&empty)), None);
        assert_eq!(documents_dir_from(env(&[])), None);
    }

    #[cfg(windows)]
    #[test]
    fn windows_test_folders_are_separate_from_the_desktop_app() {
        assert_eq!(documents_dir_from(env(&[("USERPROFILE", "C:\\U")])), Some(PathBuf::from("C:\\U").join("Documents").join("CADCraft iPad")));
        assert_eq!(support_dir_from(env(&[("APPDATA", "C:\\A")])), Some(PathBuf::from("C:\\A").join("CADCraft iPad")));
    }

    #[test]
    fn drawings_are_recognised_by_extension_in_any_case() {
        for yes in ["a.dwg", "B.DXF", "c.Dwt", "d.svg", "e.svgz", "dir/f.dxf"] {
            assert!(is_drawing(Path::new(yes)), "{yes}");
        }
        for no in ["a.txt", "dwg", "a.dwg.bak", "", ".dxf"] {
            assert!(!is_drawing(Path::new(no)), "{no}");
        }
    }

    #[test]
    fn saves_stay_inside_the_documents_folder() {
        let dir = Path::new("docs");
        assert_eq!(save_path(dir, "Plan.dwg"), dir.join("Plan.dwg"));
        assert_eq!(save_path(dir, "/private/var/mobile/Containers/x/Documents/Plan.dxf"), dir.join("Plan.dxf"));
        assert_eq!(save_path(dir, "C:\\Users\\u\\Plan.dxf"), dir.join("Plan.dxf"));
        assert_eq!(save_path(dir, "../../etc/passwd"), dir.join("passwd"));
        for hostile in ["", "  ", ".", "..", "a/..", "a/", "\\"] {
            assert_eq!(save_path(dir, hostile), dir.join(FALLBACK_NAME), "{hostile:?}");
        }
    }

    #[test]
    fn listing_shows_only_drawings_sorted_by_name() {
        let dir = std::env::temp_dir().join(format!("cadcraft-ios-list-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub.dxf")).unwrap();
        for name in ["b.dxf", "A.dwg", "notes.txt", "c.SVG"] {
            std::fs::write(dir.join(name), b"0123").unwrap();
        }
        let names: Vec<_> = list(&dir).unwrap().into_iter().map(|e| (e.name, e.bytes)).collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(names, [("A.dwg".to_string(), 4), ("b.dxf".to_string(), 4), ("c.SVG".to_string(), 4)]);
        assert!(list(&dir).is_err(), "a missing folder is an error, not a panic");
    }

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(human_size(0), "0 bytes");
        assert_eq!(human_size(820), "820 bytes");
        assert_eq!(human_size(14_540), "14.2 KB");
        assert_eq!(human_size(3_250_586), "3.1 MB");
        assert_eq!(human_size(u64::MAX), "17179869184.0 GB");
    }
}
