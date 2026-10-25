//! Folders in the app's sandbox. iPadOS sets `HOME` to the app's container; everything CADCraft
//! keeps lives under it.

use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct Paths {
    /// `Documents`: the user's drawings, shown in the Files app.
    pub documents: PathBuf,
    /// `Library/Application Support/CADCraft`: preferences (`app.ron`); backed up, not shown.
    pub settings: PathBuf,
    /// `<settings>/autosave`: autosave files for Drawing Recovery.
    pub autosave: PathBuf,
    /// `<settings>/support`: startup LISP (`acad.lsp`, `acaddoc.lsp`).
    pub support: PathBuf,
}

impl Paths {
    /// The folders under `home`; `None` when it is unset, empty or relative (never the working
    /// directory).
    pub fn from_home(home: Option<OsString>) -> Option<Self> {
        let home = PathBuf::from(home.filter(|h| !h.is_empty())?);
        if !home.is_absolute() {
            return None;
        }
        let settings = home.join("Library").join("Application Support").join("CADCraft");
        Some(Self { documents: home.join("Documents"), autosave: settings.join("autosave"), support: settings.join("support"), settings })
    }
}

#[cfg(test)]
mod tests {
    use super::Paths;
    use std::path::PathBuf;

    #[test]
    fn folders_are_under_the_container() {
        let p = Paths::from_home(Some("/var/mobile/Containers/Data/Application/X".into())).expect("absolute home");
        let home = PathBuf::from("/var/mobile/Containers/Data/Application/X");
        assert_eq!(p.documents, home.join("Documents"));
        assert_eq!(p.settings, home.join("Library/Application Support/CADCraft"));
        assert_eq!(p.autosave, p.settings.join("autosave"));
        assert_eq!(p.support, p.settings.join("support"));
    }

    #[test]
    fn no_usable_home_means_no_folders() {
        assert_eq!(Paths::from_home(None), None);
        assert_eq!(Paths::from_home(Some("".into())), None);
        assert_eq!(Paths::from_home(Some("relative/home".into())), None);
    }
}
