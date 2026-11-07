//! Block reference clips (XCLIP) in DXF (DXF Reference: SPATIAL_FILTER object). An INSERT's
//! extension dictionary holds an `ACAD_FILTER` dictionary whose `SPATIAL` entry is a
//! SPATIAL_FILTER: the boundary's points, whether clipping is on, and two 3 × 4 matrices. The
//! points are in the coordinates the reference had when it was clipped; the first matrix (the
//! inverse of the reference's transform then) takes them, after the second (the boundary's own
//! transform), into the block's coordinates, where CADCraft keeps the boundary
//! ([`cadcraft_doc::BlockClip`]).
//!
//! An inverted clip is written as its kept region: a frame around the block with the boundary
//! cut out (one even-odd outline), which every program draws as the inverted clip.

use std::collections::HashMap;
use std::sync::Arc;

use cadcraft_doc::{BlockClip, Drawing, EntityKind, EntityStore, Handle};
use cadcraft_dxf::{Section, Tag};
use cadcraft_geom::Vec2;

/// Most boundary points read per clip (hostile files).
const MAX_POINTS: usize = 100_000;

fn each_record<'a>(tags: &'a [Tag], mut f: impl FnMut(&str, &'a [Tag])) {
    let mut open: Option<(String, usize)> = None;
    for (i, t) in tags.iter().enumerate() {
        if t.code == 0 {
            if let Some((k, s)) = open.take() {
                f(&k, tags.get(s..i).unwrap_or(&[]));
            }
            open = Some((t.str(), i + 1));
        }
    }
    if let Some((k, s)) = open {
        f(&k, tags.get(s..).unwrap_or(&[]));
    }
}

fn key(s: &str) -> String {
    s.trim().to_ascii_uppercase()
}

/// A record's extension dictionary handle (`102 {ACAD_XDICTIONARY` … `360` … `102 }`).
fn xdict(tags: &[Tag]) -> Option<String> {
    let mut inside = false;
    for t in tags {
        match t.code {
            102 if t.str().trim().eq_ignore_ascii_case("{ACAD_XDICTIONARY") => inside = true,
            102 => inside = false,
            360 if inside => return Some(key(&t.str())),
            _ => {}
        }
    }
    None
}

/// A dictionary's entries: upper-case name → upper-case handle.
fn entries(tags: &[Tag]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut name: Option<String> = None;
    for t in tags {
        match t.code {
            3 => name = Some(t.str().trim().to_ascii_uppercase()),
            350 | 360 => {
                if let Some(n) = name.take() {
                    out.insert(n, key(&t.str()));
                }
            }
            _ => {}
        }
    }
    out
}

/// A 3 × 4 matrix (rows of x, y, z; the last column is the translation).
type M34 = [[f64; 4]; 3];

const IDENTITY: M34 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];

fn apply(m: &M34, p: Vec2) -> Vec2 {
    Vec2::new(m[0][0] * p.x + m[0][1] * p.y + m[0][3], m[1][0] * p.x + m[1][1] * p.y + m[1][3])
}

/// A SPATIAL_FILTER as a clip in its own coordinates and the matrix into the block's.
fn parse_filter(tags: &[Tag]) -> Option<BlockClip> {
    let start = tags.iter().position(|t| t.code == 100 && t.str() == "AcDbSpatialFilter").map_or(0, |i| i + 1);
    let tags = tags.get(start..)?;
    let mut pts: Vec<Vec2> = Vec::new();
    let mut enabled = true;
    let mut doubles: Vec<f64> = Vec::new();
    let (mut front, mut back) = (false, false);
    let mut i = 0;
    while let Some(t) = tags.get(i) {
        match t.code {
            10 => {
                let y = tags.get(i + 1).filter(|n| n.code == 20).map(Tag::f64).unwrap_or(0.0);
                if pts.len() < MAX_POINTS {
                    pts.push(Vec2::new(t.f64(), y));
                }
            }
            71 => enabled = t.i64() != 0,
            72 => front = t.i64() != 0,
            73 => back = t.i64() != 0,
            // The optional front (after 72) and back (41, after 73) clip distances, then the two
            // matrices as 24 group-40 values.
            40 => doubles.push(t.f64()),
            _ => {}
        }
        i += 1;
    }
    // A front clip distance is a 40 before the matrices; the back one is group 41.
    if front && doubles.len() == 25 {
        doubles.remove(0);
    }
    let _ = back;
    let matrix = |at: usize| -> Option<M34> {
        let v = doubles.get(at..at + 12)?;
        let mut m = IDENTITY;
        // Column-major: four columns of three rows.
        for col in 0..4 {
            for (row, line) in m.iter_mut().enumerate() {
                if let (Some(cell), Some(x)) = (line.get_mut(col), v.get(col * 3 + row)) {
                    *cell = *x;
                }
            }
        }
        m.iter().flatten().all(|x| x.is_finite()).then_some(m)
    };
    let inverse_block = matrix(0).unwrap_or(IDENTITY);
    let bound = matrix(12).unwrap_or(IDENTITY);
    // Two points: the opposite corners of a rectangle.
    if let [a, c] = pts.as_slice() {
        pts = vec![*a, Vec2::new(c.x, a.y), *c, Vec2::new(a.x, c.y)];
    }
    let boundary: Vec<Vec2> = pts.iter().map(|p| apply(&inverse_block, apply(&bound, *p))).filter(|p| p.is_finite()).collect();
    (boundary.len() >= 3).then_some(BlockClip { boundary, enabled, inverted: false })
}

/// Give the block references of the file their clips.
pub(crate) fn resolve(secs: &[Section], d: &mut Drawing) {
    // INSERT handle → its extension dictionary.
    let mut xdicts: HashMap<Handle, String> = HashMap::new();
    for s in secs.iter().filter(|s| s.name == "ENTITIES" || s.name == "BLOCKS") {
        each_record(&s.tags, |kind, tags| {
            if kind != "INSERT" {
                return;
            }
            let h = tags.iter().find(|t| t.code == 5).and_then(|t| Handle::parse_hex(&t.str()));
            if let (Some(h), Some(x)) = (h, xdict(tags)) {
                xdicts.insert(h, x);
            }
        });
    }
    if xdicts.is_empty() {
        return;
    }
    let mut dicts: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut filters: HashMap<String, BlockClip> = HashMap::new();
    for s in secs.iter().filter(|s| s.name == "OBJECTS") {
        each_record(&s.tags, |kind, tags| {
            let Some(h) = tags.iter().find(|t| t.code == 5).map(|t| key(&t.str())) else { return };
            match kind {
                "DICTIONARY" => {
                    dicts.insert(h, entries(tags));
                }
                "SPATIAL_FILTER" => {
                    if let Some(c) = parse_filter(tags) {
                        filters.insert(h, c);
                    }
                }
                _ => {}
            }
        });
    }
    let clips: HashMap<Handle, BlockClip> = xdicts
        .into_iter()
        .filter_map(|(ins, xd)| {
            let filter = dicts.get(&xd)?.get("ACAD_FILTER")?;
            let spatial = dicts.get(filter)?.get("SPATIAL")?;
            Some((ins, filters.get(spatial)?.clone()))
        })
        .collect();
    if clips.is_empty() {
        return;
    }
    fill(&mut d.model, &clips);
    for l in &mut d.layouts {
        fill(&mut l.entities, &clips);
    }
    for b in d.blocks.values_mut() {
        if b.entities.iter().any(|e| clips.contains_key(&e.handle)) {
            fill(&mut Arc::make_mut(b).entities, &clips);
        }
    }
}

fn fill(store: &mut EntityStore, clips: &HashMap<Handle, BlockClip>) {
    let hs: Vec<Handle> =
        store.iter().filter(|e| matches!(e.kind, EntityKind::Insert(_)) && clips.contains_key(&e.handle)).map(|e| e.handle).collect();
    for h in hs {
        let Some(c) = clips.get(&h) else { continue };
        store.modify(h, |e| {
            if let EntityKind::Insert(i) = &mut e.kind {
                i.clip = Some(c.clone());
            }
        });
    }
}

/// The boundary a SPATIAL_FILTER holds for `clip` on a reference whose block-to-world transform
/// is `m`: the points in world coordinates (an inverted clip as its kept region, see the module
/// comment), and the inverse of `m` as the filter's first matrix (column-major values).
pub(crate) fn filter_data(clip: &BlockClip, m: &cadcraft_geom::Mat3, block_extent: Option<(Vec2, Vec2)>) -> (Vec<Vec2>, [f64; 12]) {
    let world: Vec<Vec2> = clip.boundary.iter().map(|p| m.apply(*p)).collect();
    let pts = if clip.inverted {
        let (lo, hi) = block_extent.map_or_else(
            || {
                let b = cadcraft_geom::Bounds2::from_points(world.iter().copied());
                (b.min, b.max)
            },
            |(a, b)| (m.apply(a), m.apply(b)),
        );
        let bb = cadcraft_geom::Bounds2::from_points(world.iter().copied().chain([lo, hi]));
        let pad = bb.width().max(bb.height()) * 0.01 + 1e-6;
        let (lo, hi) = (bb.min - Vec2::new(pad, pad), bb.max + Vec2::new(pad, pad));
        let frame = [lo, Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y), lo];
        let first = world.first().copied().unwrap_or(lo);
        frame.iter().copied().chain(world.iter().copied()).chain([first]).collect()
    } else {
        world
    };
    // The inverse of the 2D affine `m` (z unchanged).
    let (a, b, c) = (m.apply(Vec2::ZERO), m.apply(Vec2::new(1.0, 0.0)), m.apply(Vec2::new(0.0, 1.0)));
    let (ux, vy) = (b - a, c - a);
    let det = ux.x * vy.y - ux.y * vy.x;
    let inv = if det.abs() > 1e-300 && det.is_finite() {
        let (i00, i01, i10, i11) = (vy.y / det, -vy.x / det, -ux.y / det, ux.x / det);
        let tx = -(i00 * a.x + i01 * a.y);
        let ty = -(i10 * a.x + i11 * a.y);
        [[i00, i01, 0.0, tx], [i10, i11, 0.0, ty], [0.0, 0.0, 1.0, 0.0]]
    } else {
        IDENTITY
    };
    let mut col_major = [0.0; 12];
    for col in 0..4 {
        for (row, line) in inv.iter().enumerate() {
            if let (Some(slot), Some(v)) = (col_major.get_mut(col * 3 + row), line.get(col)) {
                *slot = *v;
            }
        }
    }
    (pts, col_major)
}
