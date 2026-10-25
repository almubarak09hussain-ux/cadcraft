//! `log::` records to standard error, which the simulator's console (`simctl launch --console`)
//! and Xcode show. Without a logger the iPad app's start-up errors (no GPU adapter, a surface
//! that can't be configured) vanished: the first simulator runs only showed the app quitting.
//!
//! Levels: `info` for CADCraft's own crates, `warn` for everything else (wgpu and naga are
//! chatty). A record is formatted, then written to standard error in one call; writing never
//! panics.

use std::io::Write;

use log::{Level, LevelFilter, Metadata, Record};

struct Stderr;

/// The level kept for a record from `target`.
fn max_level(target: &str) -> LevelFilter {
    if target.starts_with("cadcraft") { LevelFilter::Info } else { LevelFilter::Warn }
}

impl log::Log for Stderr {
    fn enabled(&self, m: &Metadata) -> bool {
        m.level() <= max_level(m.target())
    }

    fn log(&self, r: &Record) {
        if self.enabled(r.metadata()) {
            let line = format!("[{} {}] {}\n", level_name(r.level()), r.target(), r.args());
            let _ = std::io::stderr().write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
    }
}

fn level_name(l: Level) -> &'static str {
    match l {
        Level::Error => "ERROR",
        Level::Warn => "WARN",
        Level::Info => "INFO",
        Level::Debug => "DEBUG",
        Level::Trace => "TRACE",
    }
}

static LOGGER: Stderr = Stderr;

/// Install the logger (once; a second call keeps the first).
pub fn install() {
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(LevelFilter::Info);
    }
}

#[cfg(test)]
mod tests {
    use super::{LevelFilter, max_level};

    #[test]
    fn own_crates_log_info_and_the_rest_warnings() {
        assert_eq!(max_level("cadcraft_ui_egui::gpu"), LevelFilter::Info);
        assert_eq!(max_level("cadcraft_ios"), LevelFilter::Info);
        assert_eq!(max_level("wgpu_core::device"), LevelFilter::Warn);
        assert_eq!(max_level("eframe"), LevelFilter::Warn);
    }
}
