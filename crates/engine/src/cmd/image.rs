//! Raster images: IMAGELOAD reads a PNG or JPEG picture into the drawing's pictures
//! ([`cadcraft_doc::Drawing::rasters`]); IMAGEATTACH places an image that references a loaded
//! picture by its file name, as AutoCAD's images reference their files.
//!
//! The drawing file keeps only the reference (an IMAGE and its IMAGEDEF with the file's path), so
//! a reopened drawing shows its images as frames until their pictures are loaded again
//! (IMAGELOAD with the same file name, or found next to the drawing on the desktop).

use std::sync::Arc;

use cadcraft_doc::raster::{MAX_RASTER_PIXELS, Raster, raster_key};
use cadcraft_doc::{EntityKind, Image};
use cadcraft_geom::{Vec2, Vec3};
use serde_json::{Value, json};

use super::*;
use crate::{Accept, Input, Interactive, Prompt, Result, Session, Step};

/// Largest picture file read (bytes), before decoding.
const MAX_FILE_BYTES: usize = 256 << 20;
/// Largest side of a picture, in pixels.
const MAX_SIDE: u32 = 16_384;

pub(super) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("imageload", "Load Image File", run_load)
            .params("{data: base64 PNG or JPEG bytes | path (not on the web), name?: the file's name (default: the path's)} → {name, width, height, shown: how many images in the drawing now show it}")
            .noundo(),
        CommandSpec::new("imageattach", "Raster Image Reference...", run_attach)
            .menu(&["Insert", "Raster Image Reference..."])
            .alias(&["iat"])
            .params("{name: a loaded picture's file name (IMAGELOAD) | data + name, at: [x,y], scale?: the image's width in drawing units (default 1), rotation?: degrees} → {handle} (typed or from the menu: pick a picture file)")
            .interactive(|_| Ok(Box::new(AttachM::default()))),
    ]
}

/// Decode a PNG or JPEG picture, refusing ones too large to keep.
pub fn decode(bytes: &[u8]) -> std::result::Result<Raster, String> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(format!("the picture file is larger than {} MB", MAX_FILE_BYTES >> 20));
    }
    let format = ::image::guess_format(bytes).map_err(|_| "this is not a PNG or JPEG picture".to_string())?;
    if !matches!(format, ::image::ImageFormat::Png | ::image::ImageFormat::Jpeg) {
        return Err("only PNG and JPEG pictures can be attached".into());
    }
    let mut reader = ::image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = ::image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_RASTER_PIXELS * 8);
    reader.limits(limits);
    let rgba = reader.decode().map_err(|e| format!("the picture could not be read: {e}"))?.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut r = Raster::new(w, h, rgba.into_raw()).ok_or_else(|| "the picture is too large".to_string())?;
    if format == ::image::ImageFormat::Jpeg {
        r.jpeg = Some(bytes.to_vec());
    }
    Ok(r)
}

/// Load a picture into the drawing's pictures under file name `name`; returns its key.
pub fn load(s: &mut Session, name: &str, bytes: &[u8]) -> Result<(String, Arc<Raster>)> {
    let key = raster_key(name);
    if key.is_empty() {
        return Err(bad("imageload", "the picture needs a file name"));
    }
    let r = Arc::new(decode(bytes).map_err(|e| bad("imageload", e))?);
    s.doc()?.rasters.insert(&key, r.clone());
    // The file as named (a full path on the desktop): what the image will reference.
    s.last_image = Some(name.to_string());
    // The display lists don't know the picture arrived: draw again.
    s.touch_view();
    Ok((key, r))
}

fn run_load(s: &mut Session, p: &Value) -> Result<Value> {
    let (name, bytes) = picture_param(p, "imageload")?;
    let (key, r) = load(s, &name, &bytes)?;
    let d = s.doc()?;
    let shown = d
        .model
        .iter()
        .chain(d.layouts.iter().flat_map(|l| l.entities.iter()))
        .filter(|e| matches!(&e.kind, EntityKind::Image(i) if raster_key(&i.path) == key))
        .count();
    Ok(json!({ "name": key, "width": r.width, "height": r.height, "shown": shown }))
}

/// The picture file a call names: `data` (base64) with `name`, or `path` on the desktop.
fn picture_param(p: &Value, cmd: &str) -> Result<(String, Vec<u8>)> {
    if let Some(data) = str_param(p, "data") {
        let bytes = cadcraft_engine_base64(data).ok_or_else(|| bad(cmd, "`data` is not base64"))?;
        let name = str_param(p, "name").ok_or_else(|| bad(cmd, "`name` (the file's name) is required with `data`"))?;
        return Ok((name.to_string(), bytes));
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = str_param(p, "path") {
        let meta = std::fs::metadata(path).map_err(|e| bad(cmd, format!("{path}: {e}")))?;
        if meta.len() > MAX_FILE_BYTES as u64 {
            return Err(bad(cmd, format!("{path}: the picture file is larger than {} MB", MAX_FILE_BYTES >> 20)));
        }
        let bytes = std::fs::read(path).map_err(|e| bad(cmd, format!("{path}: {e}")))?;
        return Ok((str_param(p, "name").unwrap_or(path).to_string(), bytes));
    }
    Err(bad(cmd, "`data` and `name`, or `path`, are required"))
}

fn cadcraft_engine_base64(s: &str) -> Option<Vec<u8>> {
    super::file::base64_decode(s)
}

/// An image of picture file `file` (`w` × `h` pixels) at `at`, `width` drawing units wide, turned by
/// `rotation` radians: square pixels, the picture's lower-left corner at `at`.
fn placed(file: &str, w: u32, h: u32, at: Vec2, width: f64, rotation: f64) -> Image {
    let px = width / f64::from(w.max(1));
    let (sin, cos) = rotation.sin_cos();
    let name = raster_key(file);
    let stem = std::path::Path::new(&name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(name);
    Image {
        insert: at.to3(0.0),
        u: Vec3::new(px * cos, px * sin, 0.0),
        v: Vec3::new(-px * sin, px * cos, 0.0),
        size: Vec2::new(f64::from(w), f64::from(h)),
        path: file.to_string(),
        name: stem,
        ..Image::default()
    }
}

fn run_attach(s: &mut Session, p: &Value) -> Result<Value> {
    let file = if str_param(p, "data").is_some() {
        let (name, bytes) = picture_param(p, "imageattach")?;
        load(s, &name, &bytes)?;
        name
    } else {
        str_param(p, "name").ok_or_else(|| bad("imageattach", "`name` (a loaded picture's file name) is required"))?.to_string()
    };
    let r = s.doc()?.rasters.get(&file).ok_or_else(|| bad("imageattach", format!("no picture `{file}` is loaded (IMAGELOAD it first)")))?;
    let at = point_req("imageattach", p, "at")?;
    let width = f64_or(p, "scale", 1.0);
    if !(width.is_finite() && width > 0.0) {
        return Err(bad("imageattach", "`scale` must be a positive width"));
    }
    let rotation = f64_or(p, "rotation", 0.0).to_radians();
    if !rotation.is_finite() {
        return Err(bad("imageattach", "bad `rotation`"));
    }
    let h = s.add_entity(EntityKind::Image(placed(&file, r.width, r.height, at, width, rotation)))?;
    Ok(json!({ "handle": h.hex() }))
}

/// IMAGEATTACH at the command line: the picture's name (the one just loaded by default), then
/// insertion point, scale (the width, or a point at that distance) and rotation.
#[derive(Default)]
struct AttachM {
    key: Option<(String, u32, u32)>,
    at: Option<Vec2>,
    width: Option<f64>,
}

impl AttachM {
    fn image(&self, at: Vec2, width: f64, rotation: f64) -> Option<EntityKind> {
        let (key, w, h) = self.key.as_ref()?;
        Some(EntityKind::Image(placed(key, *w, *h, at, width, rotation)))
    }

    fn finish(&mut self, s: &mut Session, rotation: f64) -> Result<Step> {
        let (Some(at), Some(width)) = (self.at, self.width) else { return Ok(Step::Continue) };
        if let Some(k) = self.image(at, width, rotation) {
            s.add_entity(k)?;
        }
        Ok(Step::Done)
    }
}

impl Interactive for AttachM {
    fn name(&self) -> &'static str {
        "IMAGEATTACH"
    }
    fn prompt(&self, s: &Session) -> Prompt {
        match (&self.key, self.at, self.width) {
            (None, ..) => {
                let p = Prompt::new("Enter the image's file name", Accept::TEXT);
                match &s.last_image {
                    Some(k) => p.default(k.clone()),
                    None => p,
                }
            }
            (Some(_), None, _) => Prompt::new("Specify insertion point", Accept::POINT),
            (Some(_), Some(at), None) => Prompt::new("Specify scale factor (the image's width)", Accept::POINT_OR_NUMBER).default("1").base(at),
            (Some(_), Some(at), Some(_)) => Prompt::new("Specify rotation angle", Accept::POINT_OR_NUMBER).default("0").base(at),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.key.is_none() {
            let name = match i {
                Input::Text(t) | Input::Keyword(t) if !t.trim().is_empty() => t,
                Input::Enter | Input::Text(_) => {
                    s.last_image.clone().ok_or_else(|| other("Load a picture first (Insert ▸ Raster Image Reference)."))?
                }
                _ => return Err(other("Enter the image's file name.")),
            };
            let r = s.doc()?.rasters.get(&name).ok_or_else(|| other(format!("No picture named {} is loaded.", raster_key(&name))))?;
            self.key = Some((name, r.width, r.height));
            return Ok(Step::Continue);
        }
        match (self.at, self.width, i) {
            (None, _, Input::Point(p)) => {
                self.at = Some(p);
                Ok(Step::Continue)
            }
            (Some(at), None, i) => {
                let w = match i {
                    Input::Point(p) => p.dist(at),
                    Input::Text(t) => t.trim().parse::<f64>().map_err(|_| other("Enter a positive number or pick a point."))?,
                    Input::Enter => 1.0,
                    _ => return Ok(Step::Continue),
                };
                if !(w.is_finite() && w > 1e-12) {
                    return Err(other("The scale must be positive."));
                }
                self.width = Some(w);
                Ok(Step::Continue)
            }
            (Some(at), Some(_), i) => {
                let r = match i {
                    Input::Point(p) => (p - at).angle(),
                    Input::Text(t) => t.trim().parse::<f64>().map_err(|_| other("Enter an angle or pick a point."))?.to_radians(),
                    Input::Enter => 0.0,
                    _ => return Ok(Step::Continue),
                };
                if !r.is_finite() {
                    return Err(other("Enter an angle or pick a point."));
                }
                self.finish(s, r)
            }
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, _s: &Session, c: Vec2) -> Vec<EntityKind> {
        let shape = match (self.at, self.width) {
            (None, _) => self.image(c, 1.0, 0.0),
            (Some(at), None) => self.image(at, c.dist(at).max(1e-9), 0.0),
            (Some(at), Some(w)) => self.image(at, w, (c - at).angle()),
        };
        shape.into_iter().collect()
    }
}

/// Load the pictures of a drawing just opened from `path` (not on the web): each image's file as
/// saved, or by its name in the drawing's folder, as AutoCAD looks for them. A picture that isn't
/// found leaves its image a frame; the command line says which.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn load_beside(s: &mut Session, path: &str) {
    let Ok(d) = s.doc() else { return };
    let dir = std::path::Path::new(path).parent().map(std::path::Path::to_path_buf).unwrap_or_default();
    let mut wanted: Vec<String> = d
        .model
        .iter()
        .chain(d.layouts.iter().flat_map(|l| l.entities.iter()))
        .chain(d.blocks.values().flat_map(|b| b.entities.iter()))
        .filter_map(|e| if let EntityKind::Image(i) = &e.kind { Some(i.path.clone()) } else { None })
        .filter(|p| !p.trim().is_empty() && !d.rasters.contains(p))
        .collect();
    wanted.sort();
    wanted.dedup();
    for file in wanted.into_iter().take(1000) {
        let as_saved = std::path::PathBuf::from(&file);
        let name = file.rsplit(['/', '\\']).next().unwrap_or(&file).to_string();
        let found = [as_saved.clone(), dir.join(&as_saved), dir.join(&name)].into_iter().find(|p| p.is_file());
        let loaded = found.and_then(|p| std::fs::read(p).ok()).and_then(|bytes| load(s, &file, &bytes).ok());
        if loaded.is_none() {
            s.echo(format!("Image file not found: {file} (Insert ▸ Reload Image Files loads it)"));
        }
    }
}

fn other(msg: impl Into<String>) -> crate::EngineError {
    crate::EngineError::Other(msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4 × 2 PNG.
    fn png() -> Vec<u8> {
        let img = ::image::RgbaImage::from_fn(4, 2, |x, y| ::image::Rgba([x as u8 * 60, y as u8 * 100, 0, 255]));
        let mut out = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut out), ::image::ImageFormat::Png).unwrap();
        out
    }

    #[test]
    fn attach_places_an_image_of_the_loaded_picture() {
        let mut s = Session::new();
        let data = super::super::file::base64_encode(&png());
        let r = s.execute("imageload", &json!({ "data": data, "name": "C:/photos/Site.PNG" })).unwrap();
        assert_eq!(r["name"], json!("site.png"));
        assert_eq!((r["width"].as_u64(), r["height"].as_u64()), (Some(4), Some(2)));
        let h = s.execute("imageattach", &json!({ "name": "site.png", "at": [10, 5], "scale": 8, "rotation": 90 })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(h["handle"].as_str().unwrap()).unwrap();
        let EntityKind::Image(i) = &s.doc().unwrap().entity(h).unwrap().kind else { panic!("not an image") };
        assert_eq!(i.path, "site.png", "the name it was attached by");
        // 8 units wide (4 pixels of 2), turned 90°: the width runs up.
        let c = i.corners();
        assert!((c[1] - Vec2::new(10.0, 13.0)).len() < 1e-9, "{c:?}");
        assert!((c[3] - Vec2::new(6.0, 5.0)).len() < 1e-9, "{c:?}");
        // It shows the picture (no crossed frame).
        let list = cadcraft_render::build(s.doc().unwrap(), &cadcraft_doc::Space::Model, &cadcraft_render::Options::default());
        assert_eq!(list.images.len(), 1);
        assert_eq!(list.images[0].tris.len(), 6);
    }

    #[test]
    fn typed_attach_asks_name_point_scale_and_rotation() {
        let mut s = Session::new();
        s.execute("imageload", &json!({ "data": super::super::file::base64_encode(&png()), "name": "logo.png" })).unwrap();
        s.start("imageattach").unwrap();
        for i in [Input::Enter, Input::Point(Vec2::new(1.0, 1.0)), Input::Text("4".into()), Input::Enter] {
            s.input(i).unwrap();
        }
        assert!(s.running.is_none());
        let d = s.doc().unwrap();
        let i = d.model.iter().find_map(|e| if let EntityKind::Image(i) = &e.kind { Some(i.clone()) } else { None }).unwrap();
        assert!((i.corners()[2] - Vec2::new(5.0, 3.0)).len() < 1e-9);
    }

    #[test]
    fn bad_pictures_are_errors_not_crashes() {
        let mut s = Session::new();
        for data in [b"not a picture".to_vec(), b"\x89PNG\r\n\x1a\n broken".to_vec(), Vec::new()] {
            let r = s.execute("imageload", &json!({ "data": super::super::file::base64_encode(&data), "name": "x.png" }));
            assert!(r.is_err());
        }
        assert!(s.execute("imageattach", &json!({ "name": "missing.png", "at": [0, 0] })).is_err());
        assert!(s.execute("imageattach", &json!({ "name": "x.png", "at": [0, 0], "scale": -1 })).is_err());
    }

    #[test]
    fn a_reopened_drawing_shows_its_images_once_the_picture_is_loaded_again() {
        let mut s = Session::new();
        let data = super::super::file::base64_encode(&png());
        s.execute("imageattach", &json!({ "data": data, "name": "plan.png", "at": [0, 0], "scale": 4 })).unwrap();
        let dxf = s.execute("export", &json!({ "format": "dxf" }));
        // Without the file formats hooked up (engine tests), check the reference itself instead.
        let _ = dxf;
        let mut d = s.doc().unwrap().clone();
        d.rasters = cadcraft_doc::raster::RasterStore::default();
        let opts = cadcraft_render::Options::default();
        assert!(cadcraft_render::build(&d, &cadcraft_doc::Space::Model, &opts).images.is_empty(), "a frame, crossed out");
        d.rasters.insert("PLAN.PNG", Arc::new(decode(&png()).unwrap()));
        assert_eq!(cadcraft_render::build(&d, &cadcraft_doc::Space::Model, &opts).images.len(), 1);
    }
}
