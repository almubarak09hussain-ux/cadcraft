//! Raster images on the canvas: a display list's images ([`cadcraft_render::DImage`]) drawn as
//! textured triangles, under its primitives (the GPU callback or the CPU shapes come after).
//!
//! Each picture becomes one texture per brightness, contrast and fade, made on first use and
//! kept while the picture stays the same; pictures larger than [`MAX_TEXTURE`] on a side are
//! scaled down for the screen (plots use the full picture).

use std::collections::HashMap;
use std::sync::Arc;

use cadcraft_doc::raster::{Raster, RasterStore, raster_key};
use cadcraft_render::DisplayList;
use egui::{Color32, Pos2};

use super::Xf;

/// Largest texture side: within every GPU's limits (WebGPU guarantees 8192).
const MAX_TEXTURE: u32 = 4096;

/// A picture's texture, with the picture it was made from (to notice a reload).
struct Entry {
    from: usize,
    texture: egui::TextureHandle,
}

/// The textures of the pictures drawn so far.
#[derive(Default)]
pub struct ImageTextures(HashMap<(String, u8, u8, u8), Entry>);

impl ImageTextures {
    fn texture(&mut self, ctx: &egui::Context, raster: &Arc<Raster>, key: &str, adjust: (u8, u8, u8)) -> Option<egui::TextureId> {
        let k = (key.to_string(), adjust.0, adjust.1, adjust.2);
        let from = Arc::as_ptr(raster) as usize;
        if let Some(e) = self.0.get(&k)
            && e.from == from
        {
            return Some(e.texture.id());
        }
        let image = color_image(raster, adjust)?;
        let texture = ctx.load_texture(format!("raster:{key}"), image, egui::TextureOptions::LINEAR);
        let id = texture.id();
        self.0.insert(k, Entry { from, texture });
        Some(id)
    }
}

/// The picture with the image's adjustments, at most [`MAX_TEXTURE`] on a side.
fn color_image(r: &Raster, (brightness, contrast, fade): (u8, u8, u8)) -> Option<egui::ColorImage> {
    let rgba = r.adjusted(brightness, contrast, fade);
    let (w, h) = (r.width, r.height);
    if w <= MAX_TEXTURE && h <= MAX_TEXTURE {
        let size = [usize::try_from(w).ok()?, usize::try_from(h).ok()?];
        return Some(egui::ColorImage::from_rgba_unmultiplied(size, &rgba));
    }
    let img = image::RgbaImage::from_raw(w, h, rgba)?;
    let k = f64::from(MAX_TEXTURE) / f64::from(w.max(h));
    let (nw, nh) = (((f64::from(w) * k) as u32).max(1), ((f64::from(h) * k) as u32).max(1));
    let small = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle);
    let size = [usize::try_from(nw).ok()?, usize::try_from(nh).ok()?];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, small.as_raw()))
}

/// Draw `list`'s images through `xf` with `p` (clipped as `p` is).
pub fn draw(textures: &mut ImageTextures, rasters: &RasterStore, p: &egui::Painter, xf: &Xf, list: &DisplayList) {
    for im in &list.images {
        let Some(raster) = rasters.get(&im.path) else { continue };
        let Some(tex) = textures.texture(p.ctx(), &raster, &raster_key(&im.path), (im.brightness, im.contrast, im.fade)) else { continue };
        let mut mesh = egui::Mesh::with_texture(tex);
        for q in &im.tris {
            let uv = im.uv(*q);
            let i = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
            mesh.vertices.push(egui::epaint::Vertex { pos: xf.to_screen(*q), uv: Pos2::new(uv.x as f32, uv.y as f32), color: Color32::WHITE });
            mesh.indices.push(i);
        }
        if mesh.is_valid() {
            p.add(egui::Shape::mesh(mesh));
        }
    }
}
