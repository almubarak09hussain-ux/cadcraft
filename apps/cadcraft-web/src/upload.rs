//! Browser uploads: File > Open… on the web, through web-sys only. There is no file system to pick
//! a path from, so the picker is a hidden `<input type="file">`; the chosen file (or one dropped on
//! the page) is read in the background and handed to the app on its next frame, which opens it
//! with OPEN `{data, name}` ([`CadApp::open_bytes`]). A DWG file is converted to DXF in the DWG
//! worker first (`dwg.rs`).

use std::cell::Cell;
use std::rc::Rc;

use cadcraft_ui_egui::CadApp;
use wasm_bindgen::JsCast as _;
use web_sys::js_sys;

/// The hidden file input (one at a time; a cancelled pick leaves it until the next one).
const INPUT_ID: &str = "cadcraft_open_input";
/// What the picker offers: the formats OPEN reads. Not used on touch devices: iPadOS knows no
/// file type for `.dwg` and would grey DWG files out in the Files picker.
const ACCEPT: &str = ".dxf,.dwg";

/// Most pictures read from one Reload Image Files pick.
const MAX_PICTURES: u32 = 100;
/// Largest picture file read, in bytes.
const MAX_PICTURE_BYTES: f64 = 256.0 * 1024.0 * 1024.0;

/// What arrived for the app: a file to open (name, contents, notes on what reading it left out),
/// a progress message, or why a file couldn't be read.
enum Upload {
    File(String, Vec<u8>, Vec<String>),
    /// A picture for a raster image (name, contents, whether it reloads images that reference it).
    Picture(String, Vec<u8>, bool),
    Progress(String),
    Failed(String),
}

/// Files read in the background, waiting for the app's next frame.
#[derive(Clone)]
pub struct Inbox {
    files: Rc<Cell<Vec<Upload>>>,
    ctx: egui::Context,
}

impl Inbox {
    pub fn new(ctx: &egui::Context) -> Self {
        Inbox { files: Rc::default(), ctx: ctx.clone() }
    }

    /// Show the browser's file picker and return at once. A chosen file arrives through
    /// [`Self::drain`]; cancelling the picker does nothing.
    pub fn pick(&self) {
        if let Err(e) = self.show_picker(None) {
            self.push(Upload::Failed(format!("Open: the browser's file picker is not available ({e})")));
        }
    }

    /// Show the picker for PNG and JPEG pictures (on an iPad it offers Photos and Files): for
    /// Insert ▸ Raster Image Reference, or several for Reload Image Files (`reload`). The pictures
    /// arrive through [`Self::drain`].
    pub fn pick_images(&self, reload: bool) {
        if let Err(e) = self.show_picker(Some(reload)) {
            self.push(Upload::Failed(format!("Image: the browser's file picker is not available ({e})")));
        }
    }

    /// `images`: pick pictures (`Some(reload)`) instead of a drawing.
    fn show_picker(&self, images: Option<bool>) -> Result<(), String> {
        let document = web_sys::window().and_then(|w| w.document()).ok_or("no browser document")?;
        let body = document.body().ok_or("the page has no body")?;
        if let Some(old) = document.get_element_by_id(INPUT_ID) {
            old.remove();
        }
        let input = document
            .create_element("input")
            .map_err(js_error)?
            .dyn_into::<web_sys::HtmlInputElement>()
            .map_err(|_| "could not create a file input".to_string())?;
        input.set_type("file");
        match images {
            // MIME types, which iPadOS understands (and offers the photo library for).
            Some(reload) => {
                input.set_accept("image/png,image/jpeg");
                input.set_multiple(reload);
            }
            None if !touch_device() => input.set_accept(ACCEPT),
            None => {}
        }
        input.set_id(INPUT_ID);
        input.set_attribute("style", "display:none").map_err(js_error)?;
        let (inbox, chosen) = (self.clone(), input.clone());
        // Called once, when a file was chosen (browsers don't fire `change` on cancel).
        let on_change = wasm_bindgen::closure::Closure::once_into_js(move || {
            let files = chosen.files();
            match images {
                Some(reload) => {
                    let n = files.as_ref().map_or(0, web_sys::FileList::length).min(MAX_PICTURES);
                    for i in 0..n {
                        if let Some(file) = files.as_ref().and_then(|f| f.get(i)) {
                            inbox.read_picture(file, reload);
                        }
                    }
                }
                None => {
                    if let Some(file) = files.and_then(|f| f.get(0)) {
                        inbox.read(file);
                    }
                }
            }
            chosen.remove();
        });
        input.set_onchange(Some(on_change.unchecked_ref()));
        body.append_child(&input).map_err(js_error)?;
        input.click();
        Ok(())
    }

    /// Read the files dropped on the page this frame (eframe hands them over unread on the web).
    pub fn take_dropped(&self, ctx: &egui::Context) {
        for f in ctx.input(|i| i.raw.dropped_files.clone()) {
            if let Some(file) = f.web_file() {
                self.read(file.clone());
            }
        }
    }

    /// Read `file` in the background; it arrives through [`Self::drain`]. A file too large to
    /// open is refused before it is read.
    fn read(&self, file: web_sys::File) {
        let name = file.name();
        // `File.size` is a whole number of bytes; anything else counts as too large.
        let size = file.size();
        let len = if size.is_finite() && size >= 0.0 { size as u64 } else { u64::MAX };
        if let Some(e) = CadApp::open_size_error(&name, len) {
            return self.push(Upload::Failed(e));
        }
        let inbox = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                Ok(buf) => inbox.arrived(name, js_sys::Uint8Array::new(&buf).to_vec()),
                Err(e) => inbox.push(Upload::Failed(format!("Open: could not read {name}: {}", js_error(e)))),
            }
        });
    }

    /// Read picture `file` in the background; it arrives through [`Self::drain`].
    fn read_picture(&self, file: web_sys::File, reload: bool) {
        let name = file.name();
        let size = file.size();
        if !(size.is_finite() && (0.0..=MAX_PICTURE_BYTES).contains(&size)) {
            return self.push(Upload::Failed(format!("Image: {name} is too large")));
        }
        let inbox = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                Ok(buf) => inbox.push(Upload::Picture(name, js_sys::Uint8Array::new(&buf).to_vec(), reload)),
                Err(e) => inbox.push(Upload::Failed(format!("Image: could not read {name}: {}", js_error(e)))),
            }
        });
    }

    /// A file's contents are here: open it on the next frame, a DWG once the DWG worker has
    /// converted it.
    fn arrived(&self, name: String, bytes: Vec<u8>) {
        // A PNG or JPEG picture (dropped on the page) is attached as a raster image.
        if bytes.starts_with(&[0x89, b'P', b'N', b'G']) || bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return self.push(Upload::Picture(name, bytes, false));
        }
        if !cadcraft_dwg::is_dwg(&bytes) {
            return self.push(Upload::File(name, bytes, Vec::new()));
        }
        self.push(Upload::Progress(format!("Open: reading {name}…")));
        let (inbox, file) = (self.clone(), name.clone());
        let started = crate::dwg::read(bytes, move |r| {
            inbox.push(match r {
                Ok((dxf, notes)) => Upload::File(file, dxf, notes),
                Err(e) => Upload::Failed(format!("Open: {file}: {e}")),
            })
        });
        if let Err(e) = started {
            self.push(Upload::Failed(format!("Open: {name}: {e}")));
        }
    }

    /// A file read elsewhere (sent by the page embedding CADCraft): opened on the next frame like
    /// a picked file, or refused when too large to open.
    pub fn receive(&self, name: String, bytes: Vec<u8>) {
        match CadApp::open_size_error(&name, bytes.len() as u64) {
            Some(e) => self.push(Upload::Failed(e)),
            None => self.arrived(name, bytes),
        }
    }

    /// A file from elsewhere refused before it was read (`error` says why).
    pub fn refuse(&self, error: String) {
        self.push(Upload::Failed(error));
    }

    fn push(&self, upload: Upload) {
        let mut files = self.files.take();
        files.push(upload);
        self.files.set(files);
        self.ctx.request_repaint();
    }

    /// Open what arrived since the last frame (or report why it couldn't be read).
    pub fn drain(&self, app: &mut CadApp) {
        for upload in self.files.take() {
            match upload {
                Upload::File(name, bytes, notes) => {
                    app.open_bytes(&name, &bytes);
                    // What the DWG read left out (the DXF it converted to reads in full).
                    for note in notes {
                        app.open_failed(note);
                    }
                }
                // On the command line only, so the status bar doesn't keep it once the file opens.
                Upload::Picture(name, bytes, reload) => app.image_picked(&name, Some(&bytes), reload),
                Upload::Progress(msg) => app.session.echo(msg),
                Upload::Failed(msg) => app.open_failed(msg),
            }
        }
    }
}

/// True on a touch device (an iPad), whose file pickers filter by file type, not extension.
fn touch_device() -> bool {
    web_sys::window().is_some_and(|w| w.navigator().max_touch_points() > 0)
}

fn js_error(e: wasm_bindgen::JsValue) -> String {
    e.as_string().unwrap_or_else(|| format!("{e:?}"))
}
