//! `log` records to standard error (Xcode's console on iOS). `RUST_LOG=debug` (or `trace`,
//! `warn`, `error`, `off`) sets the level; the default is `info`.

struct Stderr;

impl log::Log for Stderr {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }
    fn log(&self, record: &log::Record<'_>) {
        // wgpu and naga are chatty at info.
        let noisy = record.target().starts_with("wgpu") || record.target().starts_with("naga");
        if self.enabled(record.metadata()) && !(noisy && record.level() > log::Level::Warn) {
            eprintln!("[{} {}] {}", record.level(), record.target(), record.args());
        }
    }
    fn flush(&self) {}
}

static LOGGER: Stderr = Stderr;

/// Install the logger; a logger installed already (by a test harness) is kept.
pub fn install() {
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(level(std::env::var("RUST_LOG").ok().as_deref()));
    }
}

fn level(value: Option<&str>) -> log::LevelFilter {
    value.and_then(|v| v.trim().parse().ok()).unwrap_or(log::LevelFilter::Info)
}

#[cfg(test)]
mod tests {
    use super::level;

    #[test]
    fn rust_log_sets_the_level_and_bad_values_fall_back_to_info() {
        assert_eq!(level(Some("debug")), log::LevelFilter::Debug);
        assert_eq!(level(Some(" WARN ")), log::LevelFilter::Warn);
        assert_eq!(level(Some("off")), log::LevelFilter::Off);
        assert_eq!(level(Some("wgpu=debug")), log::LevelFilter::Info);
        assert_eq!(level(Some("")), log::LevelFilter::Info);
        assert_eq!(level(None), log::LevelFilter::Info);
    }
}
