//! CADCraft for iPad.
//!
//! Runs the same [`cadcraft_ui_egui::CadApp`] as the desktop and web apps, through eframe on
//! winit's UIKit backend with wgpu on Metal. It differs from the desktop shell where iPadOS does:
//!
//! - Files: drawings open from and save to the app's Documents folder, which the Files app shows
//!   as On My iPad ▸ CADCraft ([`documents`]).
//! - Lifecycle: iPadOS can end a backgrounded app without warning, so going to the background
//!   (eframe's save on suspend) autosaves unsaved drawings; Drawing Recovery offers them at the
//!   next launch if the app was ended.
//! - Settings, logs and autosave files live in the sandbox's `Library/Application Support/CADCraft`,
//!   eframe's preferences in `app.ron` there (eframe has no default storage folder on iOS).
//! - No native menu bar, file dialogs, SpaceMouse or control server.
//!
//! Build and run: `cargo xtask ios` checks it; `docs/ios.md` covers bundling, the simulator and
//! signing for a device.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(any(target_os = "ios", test))]
mod documents;
#[cfg(any(target_os = "ios", test))]
mod paths;

#[cfg(target_os = "ios")]
mod ios {
    use cadcraft_engine::Session;
    use cadcraft_ui_egui::{CadApp, Services};

    use crate::documents::{self, Picker};
    use crate::paths::Paths;

    /// Stencil bits of the window's render pass, as on the desktop (clipped layout viewports).
    const STENCIL_BITS: u8 = 8;

    struct Shell {
        cad: CadApp,
        picker: Picker,
    }

    impl eframe::App for Shell {
        fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            self.cad.logic(ctx);
            // An iPad app is never quit from inside; File ▸ Exit just closes the drawings.
            self.cad.quit_requested = false;
        }
        fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
            self.cad.raw_input_hook(raw);
            cadcraft_ui_egui::cmdline::capture_tab(ctx, raw);
        }
        fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
            self.cad.ui(ui);
            if let Some(path) = self.picker.ui(ui.ctx()) {
                self.cad.open_path(&path);
            }
        }
        /// Called on suspend (the app goes to the background) and at exit: preferences, and an
        /// autosave of unsaved drawings, since a suspended app may never be resumed.
        fn save(&mut self, storage: &mut dyn eframe::Storage) {
            storage.set_string(cadcraft_ui_egui::options::PROFILE_KEY, self.cad.profile_json());
            storage.set_string(cadcraft_ui_egui::PREFS_KEY, self.cad.prefs_json());
            storage.set_string(cadcraft_ui_egui::autosave::PREFS_KEY, cadcraft_ui_egui::autosave::prefs_json());
            let saved = self.cad.autosave_now();
            if saved > 0 {
                log::info!("suspend: autosaved {saved} drawing(s)");
            }
        }
        fn persist_egui_memory(&self) -> bool {
            false
        }
    }

    fn install_io() {
        cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
            read: |b, name| cadcraft_io::read(b, name).map_err(|e| e.to_string()),
            write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
            plot: Some(|d, space, opts| cadcraft_io::plotter::plot(d, space, opts).map_err(|e| e.to_string())),
        });
        cadcraft_engine::cmd::file::set_write_as(|d, name, o| {
            cadcraft_io::write_with(d, name, &cadcraft_io::SaveOptions { version: o.version, binary: o.binary }).map_err(|e| e.to_string())
        });
        cadcraft_engine::cmd::file::set_entity_codec(cadcraft_engine::cmd::file::EntityCodec {
            groups: cadcraft_io::entity_groups,
            entity: cadcraft_io::entity_from_groups,
        });
    }

    pub fn run() -> eframe::Result {
        let paths = Paths::from_home(std::env::var_os("HOME"));
        if let Some(p) = &paths {
            for dir in [&p.documents, &p.support] {
                if let Err(e) = std::fs::create_dir_all(dir) {
                    log::warn!("can't create {}: {e}", dir.display());
                }
            }
        }
        install_io();
        let mut options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_title("CADCraft"),
            persistence_path: paths.as_ref().map(|p| p.settings.join("app.ron")),
            stencil_buffer: STENCIL_BITS,
            ..Default::default()
        };
        // As on the desktop: large command previews outgrow wgpu's default buffer cap.
        if let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup {
            let base = std::sync::Arc::clone(&create.device_descriptor);
            create.device_descriptor = std::sync::Arc::new(move |adapter| {
                let mut d = base(adapter);
                d.required_limits.max_buffer_size = d.required_limits.max_buffer_size.max(adapter.limits().max_buffer_size);
                d
            });
        }
        eframe::run_native(
            "CADCraft",
            options,
            Box::new(move |cc| {
                let documents_dir = paths.as_ref().map(|p| p.documents.clone()).unwrap_or_default();
                let save_dir = documents_dir.clone();
                let picker = Picker::new(documents_dir);
                let services = Services {
                    pick_open: Some(picker.service()),
                    pick_save: Some(Box::new(move |name: &str| Some(documents::save_path(&save_dir, name).to_string_lossy().into_owned()))),
                };
                let mut app = CadApp::new(Session::empty(), services);
                app.system_languages = std::env::var("CADCRAFT_LOCALE").map(|s| vec![s]).unwrap_or_default();
                if let Some(profile) = cc.storage.and_then(|s| s.get_string(cadcraft_ui_egui::options::PROFILE_KEY)) {
                    app.load_profile(&profile);
                }
                if let Some(prefs) = cc.storage.and_then(|s| s.get_string(cadcraft_ui_egui::PREFS_KEY)) {
                    app.load_prefs(&prefs);
                }
                if let Some(prefs) = cc.storage.and_then(|s| s.get_string(cadcraft_ui_egui::autosave::PREFS_KEY)) {
                    cadcraft_ui_egui::autosave::load_prefs(&prefs);
                }
                if let Some(p) = &paths {
                    app.enable_autosave(&p.autosave);
                    app.session.set_lisp_support_dir(Some(p.support.clone()));
                }
                if let Some(rs) = &cc.wgpu_render_state {
                    app.set_wgpu(rs, eframe::egui_wgpu::depth_format_from_bits(0, STENCIL_BITS));
                }
                if app.session.docs.is_empty() {
                    app.session.new_drawing(false);
                }
                Ok(Box::new(Shell { cad: app, picker }))
            }),
        )
    }
}

#[cfg(target_os = "ios")]
fn main() -> eframe::Result {
    ios::run()
}

#[cfg(not(target_os = "ios"))]
fn main() {
    eprintln!("cadcraft-ios only runs on iPadOS: see docs/ios.md (`cargo xtask ios` checks the build)");
}
