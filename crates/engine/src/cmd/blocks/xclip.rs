//! XCLIP: clip block references (and external references) to a boundary, so only what lies
//! inside it shows (outside it with Invert clip); turn the clip on or off, delete it, or draw it
//! as a polyline. The boundary is kept in the block's own coordinates
//! ([`cadcraft_doc::BlockClip`]), so it follows the reference when it moves, turns or scales.

use cadcraft_doc::{BlockClip, EntityKind, Handle};
use cadcraft_geom::{Bounds2, Mat3, PolyVertex, Vec2};
use serde_json::{Value, json};

use super::super::*;
use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

/// Most boundary points (a polygon or a polyline's).
const MAX_POINTS: usize = 100_000;

pub(super) fn spec() -> CommandSpec {
    CommandSpec::new("xclip", "Clip External Reference", run)
        .menu(&["Modify", "Clip", "Xref"])
        .alias(&["xc", "clip"])
        .params(
            "{handles? (block references; default: the selection), option?: \"new\" (default) | \"on\" | \"off\" | \"delete\" | \"polyline\" (draw the boundary as a polyline), \
             points?: [[x,y]…] (a polygon, drawing coordinates) | rect?: [[x1,y1],[x2,y2]] | polyline?: handle (a closed polyline), invert?: bool} → {clipped | changed | handles}",
        )
        .interactive(|_| Ok(Box::new(XclipM::default())))
}

fn other(msg: impl Into<String>) -> EngineError {
    EngineError::Other(msg.into())
}

/// The block-to-drawing transform of reference `h` (and its block's base), if it is one.
fn reference(s: &Session, h: Handle) -> Option<(cadcraft_doc::Insert, Mat3)> {
    let d = s.doc().ok()?;
    let e = d.entity(h)?;
    let EntityKind::Insert(i) = &e.kind else { return None };
    let base = d.block(&i.block).map(|b| b.base.xy()).unwrap_or(Vec2::ZERO);
    let m = i.transform(base);
    Some((i.clone(), m))
}

/// The block references among `hs`.
fn references(s: &Session, hs: &[Handle]) -> Vec<Handle> {
    hs.iter().copied().filter(|h| reference(s, *h).is_some()).collect()
}

/// A drawing-coordinates boundary in block `m`'s coordinates; `None` when `m` can't be undone.
fn to_block(m: &Mat3, pts: &[Vec2]) -> Option<Vec<Vec2>> {
    let inv = m.inverse()?;
    let out: Vec<Vec2> = pts.iter().map(|p| inv.apply(*p)).collect();
    out.iter().all(|p| p.is_finite()).then_some(out)
}

/// The closed outline of a closed polyline (or another closed boundary object).
fn polyline_outline(s: &Session, h: Handle) -> Result<Vec<Vec2>> {
    let d = s.doc()?;
    let e = d.entity(h).ok_or_else(|| other("No such object."))?;
    cadcraft_render::clip::boundary_polygon(&e.kind).ok_or_else(|| other("Select a closed polyline."))
}

/// Set (or with `None`, delete) the clip of each reference; returns how many changed.
fn set_clip(s: &mut Session, hs: &[Handle], make: impl Fn(&Mat3, Option<&BlockClip>) -> Option<Option<BlockClip>>) -> Result<usize> {
    let mut n = 0;
    for h in hs {
        let Some((ins, m)) = reference(s, *h) else { continue };
        let Some(clip) = make(&m, ins.clip.as_ref()) else { continue };
        if curves::is_locked(s, *h) {
            continue;
        }
        s.doc_mut()?.modify_entity(*h, |e| {
            if let EntityKind::Insert(i) = &mut e.kind {
                i.clip = clip.clone();
            }
        })?;
        n += 1;
    }
    Ok(n)
}

/// A new clip from a boundary in drawing coordinates on every reference of `hs`.
fn new_clip(s: &mut Session, hs: &[Handle], world: &[Vec2], invert: bool) -> Result<usize> {
    let pts: Vec<Vec2> = world.iter().copied().take(MAX_POINTS).collect();
    let area = pts.iter().zip(pts.iter().cycle().skip(1)).map(|(a, b)| a.x * b.y - b.x * a.y).sum::<f64>().abs() / 2.0;
    if pts.len() < 3 || area.is_nan() || area <= 1e-12 {
        return Err(other("The clipping boundary must enclose an area."));
    }
    set_clip(s, hs, |m, _| to_block(m, &pts).map(|boundary| Some(BlockClip { boundary, enabled: true, inverted: invert })))
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = references(s, &targets(s, p)?);
    if hs.is_empty() {
        return Err(bad("xclip", "select block references (`handles`)"));
    }
    let option = str_param(p, "option").unwrap_or("new").to_ascii_lowercase();
    let n = match option.as_str() {
        "on" | "off" => {
            let on = option == "on";
            set_clip(s, &hs, |_, c| c.map(|c| Some(BlockClip { enabled: on, ..c.clone() })))?
        }
        "delete" => set_clip(s, &hs, |_, c| c.map(|_| None))?,
        "polyline" => {
            let mut made = Vec::new();
            for h in &hs {
                let Some((ins, m)) = reference(s, *h) else { continue };
                let Some(c) = &ins.clip else { continue };
                let vs: Vec<PolyVertex> = c.boundary.iter().map(|p| PolyVertex::new(m.apply(*p))).collect();
                made.push(s.add_entity(super::super::helpers::lwpoly(vs, true))?.hex());
            }
            return Ok(json!({ "handles": made }));
        }
        "new" => {
            let invert = bool_or(p, "invert", false);
            let world: Vec<Vec2> = if let Some(pts) = points_param(p, "points") {
                pts
            } else if let Some(r) = p.get("rect").and_then(Value::as_array) {
                let a = r.first().and_then(point_value).ok_or_else(|| bad("xclip", "`rect` is two corners"))?;
                let b = r.get(1).and_then(point_value).ok_or_else(|| bad("xclip", "`rect` is two corners"))?;
                let bb = Bounds2::new(a, b);
                vec![bb.min, Vec2::new(bb.max.x, bb.min.y), bb.max, Vec2::new(bb.min.x, bb.max.y)]
            } else if let Some(h) = p.get("polyline").and_then(curves::handle_value) {
                polyline_outline(s, h)?
            } else {
                return Err(bad("xclip", "a boundary is required: `points`, `rect` or `polyline`"));
            };
            new_clip(s, &hs, &world, invert).map_err(|e| bad("xclip", e.to_string()))?
        }
        o => return Err(bad("xclip", format!("unknown option `{o}`"))),
    };
    Ok(json!({ "changed": n }))
}

/// Where XCLIP is at the command line.
#[derive(Clone, Default)]
enum Stage {
    #[default]
    Select,
    /// "Enter clipping option [ON/OFF/Clipdepth/Delete/generate Polyline/New boundary] <New>".
    Option,
    /// "Specify clipping boundary or select invert option: [Select polyline/Polygonal/Rectangular/Invert clip] <Rectangular>".
    Boundary,
    /// Rectangular: the first corner is known when `Some`.
    Rect(Option<Vec2>),
    Polygon(Vec<Vec2>),
    Polyline,
}

#[derive(Default)]
struct XclipM {
    picked: Vec<Handle>,
    stage: Stage,
    invert: bool,
}

impl XclipM {
    fn finish(&mut self, s: &mut Session, world: &[Vec2]) -> Result<Step> {
        let hs = self.picked.clone();
        let n = new_clip(s, &hs, world, self.invert)?;
        if n == 0 {
            s.echo("No block reference could be clipped (on a locked layer?).");
        }
        Ok(Step::Done)
    }
}

impl Interactive for XclipM {
    fn name(&self) -> &'static str {
        "XCLIP"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        // Block references selected before the command (noun-verb) are the ones to clip.
        let pre = references(s, &s.selection());
        if !pre.is_empty() {
            self.picked = pre;
            self.stage = Stage::Option;
        }
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match &self.stage {
            Stage::Select => {
                let msg = if self.picked.is_empty() { "Select block references" } else { "Select block references (Enter when done)" };
                Prompt::new(msg, Accept::POINT)
            }
            Stage::Option => Prompt::new("Enter clipping option", curves::KW)
                .kw(&["ON", "OFF", "Delete", "generate Polyline", "New boundary"])
                .default("New boundary"),
            Stage::Boundary => Prompt::new("Specify clipping boundary or select invert option", curves::KW)
                .kw(&["Select polyline", "Polygonal", "Rectangular", "Invert clip"])
                .default("Rectangular"),
            Stage::Rect(None) => Prompt::new("Specify first corner", Accept::POINT),
            Stage::Rect(Some(a)) => Prompt::new("Specify opposite corner", Accept::POINT).base(*a),
            Stage::Polygon(vs) if vs.is_empty() => Prompt::new("Specify first point", Accept::POINT),
            Stage::Polygon(vs) => {
                let p = Prompt::new("Specify next point", Accept::POINT).kw(&["Undo"]);
                match vs.last() {
                    Some(b) => p.base(*b),
                    None => p,
                }
            }
            Stage::Polyline => Prompt::new("Select polyline", Accept::POINT),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match std::mem::take(&mut self.stage) {
            Stage::Select => {
                match i {
                    Input::Point(p) => match curves::pick_at(s, p).filter(|h| reference(s, *h).is_some()) {
                        Some(h) if !self.picked.contains(&h) => {
                            self.picked.push(h);
                            s.set_selection(self.picked.clone());
                        }
                        Some(_) => {}
                        None => s.echo("Select a block reference."),
                    },
                    Input::Pick(hs) => {
                        for h in references(s, &hs) {
                            if !self.picked.contains(&h) {
                                self.picked.push(h);
                            }
                        }
                        s.set_selection(self.picked.clone());
                    }
                    Input::Enter if self.picked.is_empty() => return Ok(Step::Done),
                    Input::Enter => {
                        self.stage = Stage::Option;
                        return Ok(Step::Continue);
                    }
                    _ => {}
                }
                self.stage = Stage::Select;
                Ok(Step::Continue)
            }
            Stage::Option => {
                let k = match i {
                    Input::Enter => "new".to_string(),
                    Input::Keyword(k) | Input::Text(k) => k.trim().to_ascii_lowercase(),
                    _ => {
                        self.stage = Stage::Option;
                        return Ok(Step::Continue);
                    }
                };
                let hs = json!(self.picked.iter().map(|h| h.hex()).collect::<Vec<_>>());
                let option = match k.as_str() {
                    "on" => "on",
                    "off" => "off",
                    "delete" | "d" => "delete",
                    "generate polyline" | "polyline" | "p" => "polyline",
                    "new boundary" | "new" | "n" => {
                        self.stage = Stage::Boundary;
                        return Ok(Step::Continue);
                    }
                    "clipdepth" | "c" => {
                        s.echo("Clip depth applies to 3D views; CADCraft draws in 2D.");
                        self.stage = Stage::Option;
                        return Ok(Step::Continue);
                    }
                    _ => {
                        self.stage = Stage::Option;
                        return Err(other("Enter an option."));
                    }
                };
                let r = run(s, &json!({ "handles": hs, "option": option }))?;
                if option == "delete" && r["changed"] == json!(0) {
                    s.echo("No clipping boundary to delete.");
                }
                Ok(Step::Done)
            }
            Stage::Boundary => {
                let k = match i {
                    Input::Enter => "rectangular".to_string(),
                    Input::Keyword(k) | Input::Text(k) => k.trim().to_ascii_lowercase(),
                    // A point at this prompt starts a rectangle there.
                    Input::Point(p) => {
                        self.stage = Stage::Rect(Some(p));
                        return Ok(Step::Continue);
                    }
                    _ => {
                        self.stage = Stage::Boundary;
                        return Ok(Step::Continue);
                    }
                };
                self.stage = match k.as_str() {
                    "rectangular" | "r" => Stage::Rect(None),
                    "polygonal" | "p" => Stage::Polygon(Vec::new()),
                    "select polyline" | "s" | "select" => Stage::Polyline,
                    "invert clip" | "i" | "invert" => {
                        self.invert = !self.invert;
                        s.echo(if self.invert {
                            "Inside mode - objects inside the boundary will be hidden."
                        } else {
                            "Outside mode - objects outside the boundary will be hidden."
                        });
                        Stage::Boundary
                    }
                    _ => {
                        self.stage = Stage::Boundary;
                        return Err(other("Enter an option."));
                    }
                };
                Ok(Step::Continue)
            }
            Stage::Rect(first) => match (first, i) {
                (None, Input::Point(a)) => {
                    self.stage = Stage::Rect(Some(a));
                    Ok(Step::Continue)
                }
                (Some(a), Input::Point(b)) => {
                    let bb = Bounds2::new(a, b);
                    let world = [bb.min, Vec2::new(bb.max.x, bb.min.y), bb.max, Vec2::new(bb.min.x, bb.max.y)];
                    match self.finish(s, &world) {
                        Ok(step) => Ok(step),
                        Err(e) => {
                            self.stage = Stage::Rect(None);
                            Err(e)
                        }
                    }
                }
                (first, _) => {
                    self.stage = Stage::Rect(first);
                    Ok(Step::Continue)
                }
            },
            Stage::Polygon(mut vs) => match i {
                Input::Point(p) => {
                    if vs.len() < MAX_POINTS {
                        vs.push(p);
                    }
                    self.stage = Stage::Polygon(vs);
                    Ok(Step::Continue)
                }
                Input::Keyword(k) | Input::Text(k) if k.trim().eq_ignore_ascii_case("undo") || k.trim().eq_ignore_ascii_case("u") => {
                    vs.pop();
                    self.stage = Stage::Polygon(vs);
                    Ok(Step::Continue)
                }
                Input::Enter if vs.len() >= 3 => {
                    let r = self.finish(s, &vs);
                    if r.is_err() {
                        self.stage = Stage::Polygon(vs);
                    }
                    r
                }
                Input::Enter => {
                    self.stage = Stage::Polygon(vs);
                    Err(other("A polygon needs at least three points."))
                }
                _ => {
                    self.stage = Stage::Polygon(vs);
                    Ok(Step::Continue)
                }
            },
            Stage::Polyline => {
                let h = match i {
                    Input::Point(p) => curves::pick_at(s, p),
                    Input::Pick(hs) => hs.first().copied(),
                    _ => None,
                };
                let Some(h) = h else {
                    self.stage = Stage::Polyline;
                    return Ok(Step::Continue);
                };
                let world = match polyline_outline(s, h) {
                    Ok(w) => w,
                    Err(e) => {
                        self.stage = Stage::Polyline;
                        return Err(e);
                    }
                };
                self.finish(s, &world)
            }
        }
    }
    fn preview(&self, _s: &Session, c: Vec2) -> Vec<EntityKind> {
        let poly = |pts: Vec<Vec2>, closed: bool| super::super::helpers::lwpoly(pts.into_iter().map(PolyVertex::new).collect(), closed);
        match &self.stage {
            Stage::Rect(Some(a)) => {
                let bb = Bounds2::new(*a, c);
                vec![poly(vec![bb.min, Vec2::new(bb.max.x, bb.min.y), bb.max, Vec2::new(bb.min.x, bb.max.y)], true)]
            }
            Stage::Polygon(vs) if !vs.is_empty() => vec![poly(vs.iter().copied().chain(std::iter::once(c)).collect(), true)],
            _ => Vec::new(),
        }
    }
}
