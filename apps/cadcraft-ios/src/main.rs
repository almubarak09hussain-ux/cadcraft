//! CADCraft for iPad.
//!
//! Runs the same [`cadcraft_ui_egui::CadApp`] as the desktop and web apps through eframe: winit
//! drives UIKit on iOS and wgpu draws with Metal. The same binary runs on the desktop in an
//! iPad-sized window (`cargo run -p cadcraft-ios -- --sample`), so most work is tried there first.
//!
//! Files: drawings live in the app's Documents folder, which the Files app shows as
//! On My iPad › CADCraft. File › Open lists that folder in an in-app browser and Save writes
//! there (`files.rs`). On the desktop the folder is `Documents/CADCraft iPad`, or
//! `CADCRAFT_DOCUMENTS`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod files;
mod logger;

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services};

/// Stencil bits of the window's render pass: clipped layout viewports are clipped on the GPU.
const STENCIL_BITS: u8 = 8;

/// The 13-inch iPad Pro's screen in points: the desktop test window's size.
const IPAD_POINTS: [f32; 2] = [1376.0, 1032.0];

struct Shell {
    cad: CadApp,
    browser: files::Browser,
}

impl eframe::App for Shell {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.cad.logic(ctx);
        // An iPad app never quits itself (the system closes it); on the desktop File › Exit does.
        if std::mem::take(&mut self.cad.quit_requested) && !cfg!(target_os = "ios") {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.cad.raw_input_hook(raw);
        cadcraft_ui_egui::cmdline::capture_tab(ctx, raw);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.cad.ui(ui);
        if let Some(path) = self.browser.show(ui.ctx()) {
            self.cad.open_path(&path);
        }
    }
    /// A normal exit: this run's autosave files are not needed (a crash leaves them for recovery).
    fn on_exit(&mut self) {
        self.cad.shutdown_autosave();
    }
    /// Preferences survive restarts; window and egui state are not kept.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(cadcraft_ui_egui::options::PROFILE_KEY, self.cad.profile_json());
        storage.set_string(cadcraft_ui_egui::PREFS_KEY, self.cad.prefs_json());
        storage.set_string(cadcraft_ui_egui::autosave::PREFS_KEY, cadcraft_ui_egui::autosave::prefs_json());
    }
    fn persist_egui_memory(&self) -> bool {
        false
    }
}

/// File format hooks for the engine (it stays I/O-agnostic).
fn install_io() {
    cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
        read: |b, name| cadcraft_io::read(b, name).map_err(|e| e.to_string()),
        write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
        plot: Some(|d, space, opts| cadcraft_io::plotter::plot(d, space, opts).map_err(|e| e.to_string())),
    });
    cadcraft_engine::cmd::file::set_write_as(|d, name, o| {
        cadcraft_io::write_with(d, name, &cadcraft_io::SaveOptions { version: o.version, binary: o.binary }).map_err(|e| e.to_string())
    });
    // AutoLISP's entget, entmake and entmod see entities as DXF groups.
    cadcraft_engine::cmd::file::set_entity_codec(cadcraft_engine::cmd::file::EntityCodec {
        groups: cadcraft_io::entity_groups,
        entity: cadcraft_io::entity_from_groups,
    });
}

/// No system file picker yet: Open shows the Documents browser (it reports the chosen file on a
/// later frame, so the picker itself returns none) and Save writes into Documents.
fn services(browser: &files::Browser, documents: Option<std::path::PathBuf>) -> Services {
    let picker = browser.clone();
    Services {
        pick_open: Some(Box::new(move || {
            picker.request();
            None
        })),
        pick_save: Some(Box::new(move |name: &str| documents.as_deref().map(|d| files::save_path(d, name).to_string_lossy().into_owned()))),
    }
}

/// Make `dir` (if known), logging why not; the folder is returned only when it exists.
fn ensure_dir(dir: Option<std::path::PathBuf>, what: &str) -> Option<std::path::PathBuf> {
    let dir = dir.or_else(|| {
        log::warn!("no {what} folder (set HOME)");
        None
    })?;
    match std::fs::create_dir_all(&dir) {
        Ok(()) => Some(dir),
        Err(e) => {
            log::warn!("no {what} folder {}: {e}", dir.display());
            None
        }
    }
}

fn main() -> eframe::Result {
    logger::install();
    let sample = std::env::args().skip(1).any(|a| a == "--sample");
    install_io();
    let documents = ensure_dir(files::documents_dir(), "Documents");
    let support = ensure_dir(files::support_dir(), "settings");
    let surface = cadcraft_ui_egui::gpu::surface_config(None, false);
    let mut options = eframe::NativeOptions {
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration::default().with_surface_config(surface),
        viewport: egui::ViewportBuilder::default().with_title("CADCraft").with_inner_size(IPAD_POINTS).with_min_inner_size([700.0, 500.0]),
        persist_window: false,
        // eframe keeps no state on iOS unless told where.
        persistence_path: support.as_ref().map(|d| d.join("app.ron")),
        // A stencil buffer clips the contents of clipped layout viewports on the GPU.
        stencil_buffer: STENCIL_BITS,
        ..Default::default()
    };
    // Command previews of large selections outgrow wgpu's default 256 MiB buffer cap, and the
    // validation error is fatal: ask for what the adapter supports (as the desktop app does).
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup {
        let base = std::sync::Arc::clone(&create.device_descriptor);
        create.device_descriptor = std::sync::Arc::new(move |adapter| {
            let mut d = base(adapter);
            d.required_limits.max_buffer_size = d.required_limits.max_buffer_size.max(adapter.limits().max_buffer_size);
            d
        });
    }
    log::info!("cadcraft-ios {} ({} {})", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH);
    eframe::run_native(
        "CADCraft",
        options,
        Box::new(move |cc| {
            let browser = files::Browser::new(documents.clone());
            let mut app = CadApp::new(Session::empty(), services(&browser, documents));
            app.system_languages = std::env::var("CADCRAFT_LOCALE").map(|s| vec![s]).unwrap_or_else(|_| sys_locale::get_locales().collect());
            if let Some(profile) = cc.storage.and_then(|s| s.get_string(cadcraft_ui_egui::options::PROFILE_KEY)) {
                app.load_profile(&profile);
            }
            if let Some(prefs) = cc.storage.and_then(|s| s.get_string(cadcraft_ui_egui::PREFS_KEY)) {
                app.load_prefs(&prefs);
            }
            if let Some(prefs) = cc.storage.and_then(|s| s.get_string(cadcraft_ui_egui::autosave::PREFS_KEY)) {
                cadcraft_ui_egui::autosave::load_prefs(&prefs);
            }
            // iPadOS may end the app whenever it is in the background: autosave matters more here.
            if let Some(dir) = &support {
                app.enable_autosave(&dir.join("autosave"));
            }
            if let Some(rs) = &cc.wgpu_render_state {
                app.set_wgpu(rs, eframe::egui_wgpu::depth_format_from_bits(0, STENCIL_BITS));
            }
            if sample {
                let _ = app.run("ui.sample", serde_json::json!({}));
            }
            if app.session.docs.is_empty() {
                app.session.new_drawing(false);
            }
            Ok(Box::new(Shell { cad: app, browser }))
        }),
    )
}
