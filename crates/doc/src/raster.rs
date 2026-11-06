//! The pixels of the raster images a drawing shows.
//!
//! An IMAGE entity references its picture by file, as AutoCAD's do: the drawing file keeps the
//! path, not the pixels. The pixels loaded in this session (attached, or reloaded after opening)
//! live in the drawing's [`RasterStore`], keyed by the file's name ([`raster_key`]), and are never
//! written to a file. Copies of the drawing (undo snapshots) share the store.

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// Largest picture kept, in pixels (an 8K × 8K image); bigger ones are refused before decoding.
pub const MAX_RASTER_PIXELS: u64 = 64 * 1024 * 1024;

/// A decoded picture: 8-bit RGBA rows, top row first.
#[derive(Clone, PartialEq, Eq)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// The file's own bytes when it is a JPEG, which a PDF embeds as they are.
    pub jpeg: Option<Vec<u8>>,
}

impl std::fmt::Debug for Raster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Raster({}x{})", self.width, self.height)
    }
}

impl Raster {
    /// A picture from RGBA rows; `None` when the sizes don't match or it is too large.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Option<Raster> {
        let pixels = u64::from(width) * u64::from(height);
        let ok = width > 0 && height > 0 && pixels <= MAX_RASTER_PIXELS && rgba.len() as u64 == pixels * 4;
        ok.then_some(Raster { width, height, rgba, jpeg: None })
    }

    /// The pixel at column `x`, row `y` (top row 0), or transparent outside the picture.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0; 4];
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        match self.rgba.get(i..i + 4) {
            Some(&[r, g, b, a]) => [r, g, b, a],
            _ => [0; 4],
        }
    }

    /// The picture with an image's brightness, contrast and fade (each 0..=100; 50, 50, 0 leave
    /// it as it is), blended towards white as AutoCAD fades towards the background.
    pub fn adjusted(&self, brightness: u8, contrast: u8, fade: u8) -> Vec<u8> {
        if brightness == 50 && contrast == 50 && fade == 0 {
            return self.rgba.clone();
        }
        let b = (f32::from(brightness.min(100)) - 50.0) / 50.0 * 255.0 * 0.5;
        let c = (f32::from(contrast.min(100)) / 50.0).max(0.0);
        let f = f32::from(fade.min(100)) / 100.0;
        let lut: Vec<u8> = (0..=255u16)
            .map(|v| {
                let x = (f32::from(v) - 128.0) * c + 128.0 + b;
                let x = x.clamp(0.0, 255.0);
                (x + (255.0 - x) * f).round().clamp(0.0, 255.0) as u8
            })
            .collect();
        let mut out = self.rgba.clone();
        // Colour channels only: every fourth byte is alpha.
        for (k, ch) in out.iter_mut().enumerate() {
            if k % 4 != 3 {
                *ch = lut.get(usize::from(*ch)).copied().unwrap_or(*ch);
            }
        }
        out
    }
}

/// The store key of an image file: its file name in lower case, so a picture reloaded from
/// anywhere (the iPad's Files, another folder) matches the image that references it.
pub fn raster_key(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).trim().to_lowercase()
}

/// The pictures loaded for a drawing, shared by its copies (see the module comment).
#[derive(Clone, Default)]
pub struct RasterStore(Arc<RwLock<BTreeMap<String, Arc<Raster>>>>);

impl RasterStore {
    /// The picture for the file `path` (any path or name of it), if it is loaded.
    pub fn get(&self, path: &str) -> Option<Arc<Raster>> {
        let key = raster_key(path);
        self.0.read().ok()?.get(&key).cloned()
    }

    /// Keep `raster` as the picture of file `path`, replacing one loaded before.
    pub fn insert(&self, path: &str, raster: Arc<Raster>) {
        if let Ok(mut m) = self.0.write() {
            m.insert(raster_key(path), raster);
        }
    }

    pub fn contains(&self, path: &str) -> bool {
        self.get(path).is_some()
    }

    /// The keys of the loaded pictures.
    pub fn keys(&self) -> Vec<String> {
        self.0.read().map(|m| m.keys().cloned().collect()).unwrap_or_default()
    }
}

impl std::fmt::Debug for RasterStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RasterStore({:?})", self.keys())
    }
}

/// Pixels aren't part of what makes two drawings equal (they aren't in the file either).
impl PartialEq for RasterStore {
    fn eq(&self, _: &RasterStore) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_file_names_in_any_case() {
        assert_eq!(raster_key("C:\\Plans\\Site Photo.JPG"), "site photo.jpg");
        assert_eq!(raster_key("../img/logo.png"), "logo.png");
        let store = RasterStore::default();
        let r = Arc::new(Raster::new(1, 1, vec![1, 2, 3, 255]).unwrap());
        store.insert("/a/Logo.png", r);
        assert!(store.contains("logo.PNG"));
        assert_eq!(store.get("x/LOGO.png").unwrap().pixel(0, 0), [1, 2, 3, 255]);
        // Copies share the store.
        let copy = store.clone();
        copy.insert("b.png", Arc::new(Raster::new(1, 1, vec![0; 4]).unwrap()));
        assert!(store.contains("b.png"));
    }

    #[test]
    fn bad_sizes_are_refused() {
        assert!(Raster::new(2, 2, vec![0; 12]).is_none());
        assert!(Raster::new(0, 1, Vec::new()).is_none());
        assert!(Raster::new(65_536, 65_536, Vec::new()).is_none());
        let r = Raster::new(1, 1, vec![0, 0, 0, 255]).unwrap();
        assert_eq!(r.pixel(5, 5), [0; 4]);
        // Full fade is white.
        assert_eq!(&r.adjusted(50, 50, 100)[..3], &[255, 255, 255]);
        assert_eq!(r.adjusted(50, 50, 0), r.rgba);
    }
}
