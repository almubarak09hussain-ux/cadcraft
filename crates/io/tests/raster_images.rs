//! Raster images with their pictures loaded: they plot (PDF image objects, PNG pixels), and a
//! saved drawing keeps only the reference, as AutoCAD's do.

use std::sync::Arc;

use cadcraft_doc::raster::Raster;
use cadcraft_doc::{Common, Drawing, EntityKind, Image, Space};
use cadcraft_geom::{Vec2, Vec3};

/// A drawing with a 2 × 2 red picture, 10 units wide, at the origin.
fn drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    let img = Image {
        insert: Vec3::ZERO,
        u: Vec3::new(5.0, 0.0, 0.0),
        v: Vec3::new(0.0, 5.0, 0.0),
        size: Vec2::new(2.0, 2.0),
        path: "red.png".into(),
        name: "red".into(),
        ..Image::default()
    };
    d.add(&Space::Model, Common::default(), EntityKind::Image(img)).unwrap();
    let red = Raster::new(2, 2, [255u8, 0, 0, 255].repeat(4)).unwrap();
    d.rasters.insert("red.png", Arc::new(red));
    d
}

#[test]
fn a_loaded_picture_plots_into_the_pdf() {
    let d = drawing();
    let o = cadcraft_io::PdfOptions { compress: false, ..cadcraft_io::PdfOptions::default() };
    let pdf = cadcraft_io::pdf(&d, &Space::Model, &o).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/Subtype /Image /Width 2 /Height 2"), "an image object");
    assert!(text.contains("/XObject << /Im"), "in the page's resources");
    assert!(text.contains(" Do\n"), "drawn on the page");
    // No picture loaded: the frame (crossed out) plots instead.
    let mut missing = d.clone();
    missing.rasters = Default::default();
    let pdf = cadcraft_io::pdf(&missing, &Space::Model, &o).unwrap();
    assert!(!String::from_utf8_lossy(&pdf).contains("/Subtype /Image"));
}

#[test]
fn a_loaded_picture_shows_in_png_exports() {
    let d = drawing();
    let png = cadcraft_io::write(&d, "x.png").unwrap();
    let img = tiny_skia::Pixmap::decode_png(&png).unwrap();
    let mid = img.pixel(img.width() / 2, img.height() / 2).unwrap().demultiply();
    assert!(mid.red() > 200 && mid.green() < 60 && mid.blue() < 60, "{mid:?}");
}

#[test]
fn a_saved_drawing_keeps_the_reference_not_the_pixels() {
    let d = drawing();
    let bytes = cadcraft_io::write(&d, "a.dxf").unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("IMAGEDEF") && text.contains("red.png"));
    let back = cadcraft_io::read(&bytes, "a.dxf").unwrap();
    assert!(back.rasters.keys().is_empty(), "pictures load again from their files");
    let Some(EntityKind::Image(i)) = back.model.iter().map(|e| e.kind.clone()).next() else { panic!("no image") };
    assert_eq!(i.path, "red.png");
}
