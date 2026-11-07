//! Block reference clips (XCLIP): a clipped reference draws only what lies inside its boundary
//! (outside when inverted), on screen and in plots, and the clip survives a save and reopen as
//! a SPATIAL_FILTER in the reference's extension dictionary.

use std::sync::Arc;

use cadcraft_doc::*;
use cadcraft_geom::{Vec2, Vec3};

/// Block GRID: horizontal lines y = 0..=10 from x 0 to 10; a reference at (100, 50), turned 90°.
fn drawing(clip: Option<BlockClip>) -> Drawing {
    let mut d = Drawing::new_metric();
    let mut b = Block::new("GRID");
    for k in 0..=10 {
        let y = f64::from(k);
        let line = EntityKind::Line(Line { a: Vec3::new(0.0, y, 0.0), b: Vec3::new(10.0, y, 0.0) });
        b.entities.push(Entity::new(Handle(0x9000 + u64::try_from(k).unwrap()), line));
    }
    d.blocks.insert("GRID".into(), Arc::new(b));
    let ins = Insert {
        block: "GRID".into(),
        insert: Vec3::new(100.0, 50.0, 0.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: std::f64::consts::FRAC_PI_2,
        attribs: Vec::new(),
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
        clip,
    };
    d.add(&Space::Model, Common::default(), EntityKind::Insert(ins)).unwrap();
    d
}

/// The square 2..5 × 2..5 in the block's coordinates.
fn square(inverted: bool) -> BlockClip {
    BlockClip { boundary: vec![Vec2::new(2.0, 2.0), Vec2::new(5.0, 2.0), Vec2::new(5.0, 5.0), Vec2::new(2.0, 5.0)], enabled: true, inverted }
}

/// The block point a world point of the reference came from (turned 90° about (100, 50)).
fn to_block(p: Vec2) -> Vec2 {
    Vec2::new(p.y - 50.0, 100.0 - p.x)
}

fn drawn(d: &Drawing) -> Vec<Vec2> {
    let list = cadcraft_render::build(d, &Space::Model, &cadcraft_render::Options::default());
    list.verts.clone()
}

#[test]
fn a_clipped_reference_draws_only_inside_its_boundary() {
    let pts = drawn(&drawing(Some(square(false))));
    assert!(!pts.is_empty());
    for p in &pts {
        let q = to_block(*p);
        assert!(q.x >= 2.0 - 1e-6 && q.x <= 5.0 + 1e-6 && q.y >= 2.0 - 1e-6 && q.y <= 5.0 + 1e-6, "{q:?} outside the clip");
    }
    // Clipping off: the whole block again.
    let mut off = square(false);
    off.enabled = false;
    assert!(drawn(&drawing(Some(off))).iter().any(|p| to_block(*p).x > 9.0));
}

#[test]
fn an_inverted_clip_draws_only_outside_its_boundary() {
    let pts = drawn(&drawing(Some(square(true))));
    assert!(pts.iter().any(|p| to_block(*p).x > 9.0), "the rest of the block is drawn");
    // Segments along y = 3 and y = 4 are cut where they cross the square.
    let list = cadcraft_render::build(&drawing(Some(square(true))), &Space::Model, &cadcraft_render::Options::default());
    for p in &list.prims {
        for w in list.points(p).windows(2) {
            let (a, b) = (to_block(w[0]), to_block(w[1]));
            let mid = (a + b) * 0.5;
            let inside = mid.x > 2.0 + 1e-6 && mid.x < 5.0 - 1e-6 && mid.y > 2.0 + 1e-6 && mid.y < 5.0 - 1e-6;
            assert!(!inside, "a piece inside the inverted clip: {a:?}–{b:?}");
        }
    }
}

#[test]
fn a_clip_survives_save_and_reopen() {
    let d = drawing(Some(square(false)));
    let text = cadcraft_io::write_dxf(&d);
    assert!(text.contains("SPATIAL_FILTER") && text.contains("ACAD_FILTER"));
    let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    let Some(EntityKind::Insert(i)) = back.model.iter().map(|e| e.kind.clone()).next() else { panic!("no insert") };
    let clip = i.clip.expect("the clip");
    assert!(clip.enabled && !clip.inverted);
    assert_eq!(clip.boundary.len(), 4);
    for (a, b) in clip.boundary.iter().zip(square(false).boundary) {
        assert!((*a - b).len() < 1e-6, "{a:?} vs {b:?}");
    }
    // Saved again, still one filter (no stale copy from the first file).
    let again = cadcraft_io::write_dxf(&back);
    assert_eq!(again.matches("SPATIAL_FILTER").count(), 1);
}

#[test]
fn an_inverted_clip_reopens_drawing_the_same() {
    let d = drawing(Some(square(true)));
    let back = cadcraft_io::read_dxf(cadcraft_io::write_dxf(&d).as_bytes()).unwrap();
    let pts = drawn(&back);
    assert!(pts.iter().any(|p| to_block(*p).x > 9.0));
    let list = cadcraft_render::build(&back, &Space::Model, &cadcraft_render::Options::default());
    for p in &list.prims {
        for w in list.points(p).windows(2) {
            let mid = (to_block(w[0]) + to_block(w[1])) * 0.5;
            assert!(!(mid.x > 2.1 && mid.x < 4.9 && mid.y > 2.1 && mid.y < 4.9), "drawn inside: {mid:?}");
        }
    }
}

#[test]
fn hostile_spatial_filters_do_not_panic() {
    let d = drawing(Some(square(false)));
    let text = cadcraft_io::write_dxf(&d);
    for bad in ["1e308", "nan", "-1e400", ""] {
        let t = text.replacen(" 40\r\n1.0", &format!(" 40\r\n{bad}"), 3);
        let _ = cadcraft_io::read_dxf(t.as_bytes());
    }
}

/// The SPATIAL_FILTER's group-40 values after the boundary, in file order.
fn filter_values(text: &str) -> Vec<f64> {
    let start = text.find("AcDbSpatialFilter").unwrap();
    // After the subclass marker line: code, value, code, value…
    let lines: Vec<&str> = text[start..].lines().skip(1).map(str::trim).collect();
    let mut out = Vec::new();
    for pair in lines.chunks(2) {
        if let [code, value] = pair {
            if *code == "0" {
                break;
            }
            if *code == "40" {
                out.push(value.parse().unwrap());
            }
        }
    }
    out
}

#[test]
fn the_filter_matrices_are_written_row_by_row_as_autocad_reads_them() {
    // Unturned reference at (100, 50): the first matrix (block ← world) is a move by (-100, -50),
    // its translation the fourth value of each row (AutoCAD's files have it there).
    let mut d = drawing(Some(square(false)));
    let h = d.model.iter().map(|e| e.handle).next().unwrap();
    d.model.modify(h, |e| {
        if let EntityKind::Insert(i) = &mut e.kind {
            i.rotation = 0.0;
        }
    });
    let text = cadcraft_io::write_dxf(&d);
    let v = filter_values(&text);
    assert!(v.len() >= 24, "{v:?}");
    let first = &v[v.len() - 24..v.len() - 12];
    let want = [1.0, 0.0, 0.0, -100.0, 0.0, 1.0, 0.0, -50.0, 0.0, 0.0, 1.0, 0.0];
    for (a, b) in first.iter().zip(want) {
        assert!((a - b).abs() < 1e-9, "{first:?}");
    }
    // A clip made when the reference stood elsewhere: the boundary stays where it was on the block.
    let moved = text.replacen("-100.0", "-110.0", 1);
    let back = cadcraft_io::read_dxf(moved.as_bytes()).unwrap();
    let Some(EntityKind::Insert(i)) = back.model.iter().map(|e| e.kind.clone()).next() else { panic!("no insert") };
    let b = cadcraft_geom::Bounds2::from_points(i.clip.unwrap().boundary.iter().copied());
    assert!((b.min - Vec2::new(-8.0, 2.0)).len() < 1e-6 && (b.max - Vec2::new(-5.0, 5.0)).len() < 1e-6, "{b:?}");
}
