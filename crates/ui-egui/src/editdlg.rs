//! Editing dialogs: the Enhanced Attribute Editor (a double-click on a block with attributes),
//! Multiline Edit Tools (MLEDIT, or a double-click on a multiline) and Batch Publish (PUBLISH
//! typed or from the File menu).
//!
//! Both change the drawing or write files only through commands (`attedit`, `plot`, `publish`),
//! so their edits are undoable and the same as a script's.

use egui::{RichText, vec2};
use serde_json::{Value, json};

use crate::CadApp;
use crate::theme::Tokens;

/// Dialog name prefix of the attribute editor: `attedit:<handle hex>`.
const ATTEDIT: &str = "attedit:";

/// Open the dialog for a typed or menu-invoked command. `None` if `id` has no dialog here or the
/// call carries parameters.
pub fn route(app: &mut CadApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if !params.is_null() {
        return None;
    }
    match id {
        "imageattach" | "iat" | "attach" | "image" => Some(app.pick_images(false)),
        "ui.imagereload" => Some(app.pick_images(true)),
        "publish" | "mledit" => {
            if let Err(e) = app.session.doc() {
                return Some(Err(e.to_string()));
            }
            app.ui.dialog = Some(id.into());
            Some(Ok(Value::Null))
        }
        _ => None,
    }
}

/// Show the dialog named `name`. `false` if it isn't one of these.
pub fn dialog(app: &mut CadApp, ctx: &egui::Context, name: &str, open: &mut bool) -> bool {
    if let Some(hex) = name.strip_prefix(ATTEDIT) {
        attedit(app, ctx, hex, open);
        return true;
    }
    match name {
        "publish" => publish(app, ctx, open),
        "mledit" => mledit(app, ctx, open),
        _ => return false,
    }
    true
}

// ---------- Enhanced Attribute Editor ----------

/// One attribute row: tag, prompt, value.
#[derive(Clone)]
struct Row {
    tag: String,
    prompt: String,
    value: String,
}

/// True when the block reference `h` has attributes a user can change (not all constant).
pub fn has_editable_attributes(app: &CadApp, h: cadcraft_engine::doc::Handle) -> bool {
    app.session.doc().ok().and_then(|d| d.entity(h)).is_some_and(|e| match &e.kind {
        cadcraft_engine::doc::EntityKind::Insert(i) => i.attribs.iter().any(|a| !a.constant),
        _ => false,
    })
}

/// Open the attribute editor on block reference `h`.
pub fn open_attedit(app: &mut CadApp, h: cadcraft_engine::doc::Handle) {
    app.session.set_selection(vec![h]);
    app.ui.dialog = Some(format!("{ATTEDIT}{}", h.hex()));
}

fn rows_of(app: &CadApp, h: cadcraft_engine::doc::Handle) -> Option<(String, Vec<Row>)> {
    let d = app.session.doc().ok()?;
    match &d.entity(h)?.kind {
        cadcraft_engine::doc::EntityKind::Insert(i) => Some((
            i.block.clone(),
            i.attribs
                .iter()
                .filter(|a| !a.constant)
                .map(|a| Row {
                    tag: a.tag.clone(),
                    prompt: if a.prompt.is_empty() { a.tag.clone() } else { a.prompt.clone() },
                    value: a.text.value.clone(),
                })
                .collect(),
        )),
        _ => None,
    }
}

fn attedit(app: &mut CadApp, ctx: &egui::Context, hex: &str, open: &mut bool) {
    let t = Tokens::get();
    let Some(h) = cadcraft_engine::doc::Handle::parse_hex(hex) else {
        *open = false;
        return;
    };
    let id = egui::Id::new("attedit_rows").with(hex);
    let fresh = rows_of(app, h);
    let Some((block, fresh_rows)) = fresh else {
        // The block reference is gone (undone, erased).
        *open = false;
        return;
    };
    let mut rows = ctx.data_mut(|d| d.get_temp::<Vec<Row>>(id)).unwrap_or(fresh_rows);
    let mut sel = ctx.data_mut(|d| d.get_temp::<usize>(id.with("sel"))).unwrap_or(0).min(rows.len().saturating_sub(1));
    let (mut ok, mut apply, mut cancel) = (false, false, false);
    egui::Window::new(crate::tl!("Enhanced Attribute Editor"))
        .id(egui::Id::new("Enhanced Attribute Editor"))
        .open(open)
        .collapsible(false)
        .default_size(vec2(520.0, 320.0))
        .resizable(true)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(crate::tl!("Block:"));
                ui.label(RichText::new(&block).strong());
            });
            ui.separator();
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                egui::Grid::new("attedit_grid").num_columns(3).striped(true).spacing(vec2(12.0, 6.0)).show(ui, |ui| {
                    ui.label(RichText::new(crate::tl!("Tag")).color(t.text_dim));
                    ui.label(RichText::new(crate::tl!("Prompt")).color(t.text_dim));
                    ui.label(RichText::new(crate::tl!("Value")).color(t.text_dim));
                    ui.end_row();
                    for (i, r) in rows.iter_mut().enumerate() {
                        if ui.selectable_label(i == sel, &r.tag).clicked() {
                            sel = i;
                        }
                        ui.label(&r.prompt);
                        if ui.add(egui::TextEdit::singleline(&mut r.value).desired_width(240.0)).gained_focus() {
                            sel = i;
                        }
                        ui.end_row();
                    }
                });
            });
            if rows.is_empty() {
                ui.label(crate::tl!("This block has no attributes to edit."));
            }
            ui.separator();
            ui.horizontal(|ui| {
                ok = ui.button(crate::tl!("OK")).clicked();
                apply = ui.button(crate::tl!("Apply")).clicked();
                cancel = ui.button(crate::tl!("Cancel")).clicked();
            });
        });
    ctx.data_mut(|d| d.insert_temp(id.with("sel"), sel));
    if ok || apply {
        let values: serde_json::Map<String, Value> = rows.iter().map(|r| (r.tag.clone(), json!(r.value))).collect();
        // Errors (a locked layer) are echoed on the command line by `run`.
        let _ = app.run("attedit", json!({ "handle": hex, "values": values }));
    }
    if ok || cancel || !*open {
        *open = false;
        ctx.data_mut(|d| {
            d.remove::<Vec<Row>>(id);
            d.remove::<usize>(id.with("sel"));
        });
    } else if apply {
        // Show the drawing's values again (what the command kept).
        ctx.data_mut(|d| d.remove::<Vec<Row>>(id));
    } else {
        ctx.data_mut(|d| d.insert_temp(id, std::mem::take(&mut rows)));
    }
}

// ---------- Multiline Edit Tools ----------

/// MLEDIT's tools in AutoCAD's dialog order (columns: crosses, tees, corner and vertices, cuts):
/// keyword and label.
const ML_TOOLS: [[(&str, &str); 4]; 3] = [
    [("CC", "Closed Cross"), ("CT", "Closed Tee"), ("CJ", "Corner Joint"), ("CS", "Cut Single")],
    [("OC", "Open Cross"), ("OT", "Open Tee"), ("AV", "Add Vertex"), ("CA", "Cut All")],
    [("MC", "Merged Cross"), ("MT", "Merged Tee"), ("DV", "Delete Vertex"), ("WA", "Weld All")],
];

fn mledit(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) {
    let t = Tokens::get();
    let mut picked = None;
    let mut close = false;
    egui::Window::new(crate::tl!("Multiline Edit Tools"))
        .id(egui::Id::new("Multiline Edit Tools"))
        .open(open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(RichText::new(crate::tl!("Click a tool, then pick the multilines in the drawing.")).color(t.text_dim));
            ui.add_space(4.0);
            egui::Grid::new("mledit_tools").num_columns(4).spacing(vec2(10.0, 10.0)).show(ui, |ui| {
                for row in ML_TOOLS {
                    for (kw, label) in row {
                        let (r, resp) = ui.allocate_exact_size(vec2(96.0, 84.0), egui::Sense::click());
                        let p = ui.painter();
                        p.rect_filled(r, 3.0, if resp.hovered() { t.control_hover } else { t.canvas });
                        p.rect_stroke(r, 3.0, egui::Stroke::new(1.0, t.text_dim.gamma_multiply(0.4)), egui::StrokeKind::Inside);
                        ml_tool_icon(p, egui::Rect::from_min_size(r.min + vec2(18.0, 6.0), vec2(60.0, 52.0)), kw, t.text);
                        p.text(
                            egui::pos2(r.center().x, r.bottom() - 12.0),
                            egui::Align2::CENTER_CENTER,
                            crate::i18n::t(label),
                            egui::FontId::proportional(11.0),
                            t.text,
                        );
                        if resp.on_hover_text(kw).clicked() {
                            picked = Some(kw);
                        }
                    }
                    ui.end_row();
                }
            });
            ui.separator();
            close = ui.button(crate::tl!("Close")).clicked();
        });
    if let Some(kw) = picked {
        *open = false;
        // The tool's prompts (select the multilines) follow on the command line and the canvas.
        // The engine's MLEDIT (`app.start` would open this dialog again).
        app.session.set_selection(Vec::new());
        let r = app.session.start("mledit").and_then(|()| app.session.input(cadcraft_engine::Input::Keyword(kw.into())));
        if let Err(e) = r {
            app.session.echo(e.to_string());
        }
    }
    if close {
        *open = false;
    }
}

/// A tool's picture: two multilines (pairs of lines) meeting as the tool leaves them.
fn ml_tool_icon(p: &egui::Painter, r: egui::Rect, kw: &str, c: egui::Color32) {
    use egui::pos2;
    let s = egui::Stroke::new(1.6, c);
    let (l, rt, tp, bt) = (r.left(), r.right(), r.top(), r.bottom());
    let (cx, cy) = (r.center().x, r.center().y);
    // Half the distance between a multiline's two lines.
    let g = 6.0;
    let h = |y: f32, x0: f32, x1: f32| p.line_segment([pos2(x0, y), pos2(x1, y)], s);
    let v = |x: f32, y0: f32, y1: f32| p.line_segment([pos2(x, y0), pos2(x, y1)], s);
    match kw {
        // Crosses: the vertical multiline over the horizontal one.
        "CC" => {
            v(cx - g, tp, bt);
            v(cx + g, tp, bt);
            for y in [cy - g, cy + g] {
                h(y, l, cx - g);
                h(y, cx + g, rt);
            }
        }
        "OC" | "MC" => {
            for x in [cx - g, cx + g] {
                v(x, tp, cy - g);
                v(x, cy + g, bt);
            }
            for y in [cy - g, cy + g] {
                h(y, l, cx - g);
                h(y, cx + g, rt);
            }
            if kw == "MC" {
                // The middle lines run through (merged).
                v(cx, tp + 4.0, bt - 4.0);
                h(cy, l + 4.0, rt - 4.0);
            }
        }
        // Tees: a vertical multiline ending at the horizontal one.
        "CT" => {
            h(cy - g, l, rt);
            h(cy + g, l, rt);
            v(cx - g, cy + g, bt);
            v(cx + g, cy + g, bt);
        }
        "OT" | "MT" => {
            h(cy - g, l, rt);
            h(cy + g, l, cx - g);
            h(cy + g, cx + g, rt);
            v(cx - g, cy + g, bt);
            v(cx + g, cy + g, bt);
            if kw == "MT" {
                h(cy, l + 4.0, rt - 4.0);
                v(cx, cy, bt - 4.0);
            }
        }
        // Corner joint: an L.
        "CJ" => {
            v(cx - g, cy - g, bt);
            v(cx + g, cy + g, bt);
            h(cy - g, cx - g, rt);
            h(cy + g, cx + g, rt);
        }
        // Vertices: a multiline with the vertex added (circled) or deleted (crossed out).
        "AV" | "DV" => {
            for d in [-g, g] {
                h(cy + d, l, rt);
            }
            if kw == "AV" {
                p.circle_stroke(pos2(cx, cy), 3.5, s);
            } else {
                p.line_segment([pos2(cx - 4.0, cy - 4.0), pos2(cx + 4.0, cy + 4.0)], s);
                p.line_segment([pos2(cx - 4.0, cy + 4.0), pos2(cx + 4.0, cy - 4.0)], s);
            }
        }
        // Cuts: a gap in one line or in both.
        "CS" => {
            h(cy - g, l, cx - 6.0);
            h(cy - g, cx + 6.0, rt);
            h(cy + g, l, rt);
        }
        "CA" => {
            for y in [cy - g, cy + g] {
                h(y, l, cx - 6.0);
                h(y, cx + 6.0, rt);
            }
        }
        // Weld All: the gap closed again.
        _ => {
            for y in [cy - g, cy + g] {
                h(y, l, rt);
            }
            v(cx, cy - g - 4.0, cy + g + 4.0);
        }
    }
}

// ---------- Batch Publish ----------

/// The Publish dialog's choices, kept while it is open.
#[derive(Clone)]
struct PublishState {
    /// Every sheet (Model if it has objects, then the layouts in tab order) and whether it is in.
    sheets: Vec<(String, bool)>,
    /// One multi-sheet PDF instead of one PDF per sheet.
    combined: bool,
}

fn sheet_names(app: &CadApp) -> Vec<String> {
    let Ok(d) = app.session.doc() else { return Vec::new() };
    let mut layouts: Vec<_> = d.layouts.iter().collect();
    layouts.sort_by_key(|l| l.tab_order);
    (!d.model.is_empty()).then(|| "Model".to_string()).into_iter().chain(layouts.iter().map(|l| l.name.clone())).collect()
}

fn publish(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) {
    let t = Tokens::get();
    let id = egui::Id::new("publish_state");
    let names = sheet_names(app);
    let mut st = ctx.data_mut(|d| d.get_temp::<PublishState>(id)).filter(|s| s.sheets.iter().map(|(n, _)| n).eq(names.iter())).unwrap_or_else(|| {
        // Every layout, and Model only when there are no layouts (AutoCAD's default list).
        let only_model = names.len() == 1;
        PublishState { sheets: names.iter().map(|n| (n.clone(), only_model || n != "Model")).collect(), combined: true }
    });
    let title = app.session.state().map(|s| s.title.clone()).unwrap_or_default();
    let stem = std::path::Path::new(&title)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Drawing".into());
    let (mut go, mut cancel) = (false, false);
    egui::Window::new(crate::tl!("Batch Publish"))
        .id(egui::Id::new("Batch Publish"))
        .open(open)
        .collapsible(false)
        .default_size(vec2(420.0, 360.0))
        .show(ctx, |ui| {
            ui.label(RichText::new(crate::tl!("Sheets to publish")).strong());
            ui.horizontal(|ui| {
                if ui.small_button(crate::tl!("All")).clicked() {
                    st.sheets.iter_mut().for_each(|s| s.1 = true);
                }
                if ui.small_button(crate::tl!("None")).clicked() {
                    st.sheets.iter_mut().for_each(|s| s.1 = false);
                }
                if ui.small_button(crate::tl!("Layouts only")).clicked() {
                    st.sheets.iter_mut().for_each(|s| s.1 = s.0 != "Model");
                }
            });
            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                for (name, on) in &mut st.sheets {
                    let label = if name == "Model" { format!("{stem}-{}", crate::tl!("Model")) } else { format!("{stem}-{name}") };
                    ui.checkbox(on, label);
                }
                if st.sheets.is_empty() {
                    ui.label(crate::tl!("This drawing has no sheets to publish."));
                }
            });
            ui.separator();
            ui.label(RichText::new(crate::tl!("PDF output")).strong());
            ui.radio_value(&mut st.combined, true, crate::tl!("Multi-sheet file (one PDF, a page per sheet)"));
            ui.radio_value(&mut st.combined, false, crate::tl!("A separate PDF for each sheet"));
            let count = st.sheets.iter().filter(|s| s.1).count();
            ui.label(RichText::new(crate::tl!("Each sheet plots with its page setup.")).small().color(t.text_dim));
            ui.separator();
            ui.horizontal(|ui| {
                go = ui.add_enabled(count > 0, egui::Button::new(crate::tl!("Publish"))).clicked();
                cancel = ui.button(crate::tl!("Cancel")).clicked();
            });
        });
    if go {
        let chosen: Vec<String> = st.sheets.iter().filter(|s| s.1).map(|s| s.0.clone()).collect();
        let suggested = format!("{stem}.pdf");
        // The host's file dialog (the browser: the download's name); cancelling keeps the dialog.
        let path = match app.services.pick_save.as_ref() {
            Some(pick) => match pick(&suggested) {
                Some(p) => Some(p),
                None => {
                    ctx.data_mut(|d| d.insert_temp(id, st));
                    return;
                }
            },
            None => None,
        };
        let path = path.map(|p| if p.to_ascii_lowercase().ends_with(".pdf") { p } else { format!("{p}.pdf") });
        let r = if st.combined {
            let mut p = json!({ "layouts": chosen });
            if let Some(path) = &path {
                p["path"] = json!(path);
            }
            app.run("plot", p).map(|r| match r.get("path").and_then(Value::as_str) {
                Some(path) => format!("Published {} sheet(s) to {path}", chosen.len()),
                None => format!("Published {} sheet(s) into one PDF", chosen.len()),
            })
        } else {
            let mut p = json!({ "layouts": chosen });
            if let Some(path) = &path {
                p["path"] = json!(path);
            }
            app.run("publish", p).map(|r| r["message"].as_str().map_or_else(|| format!("Published {} sheet(s)", chosen.len()), str::to_string))
        };
        // On an error (`run` echoed it) the dialog stays to change the choices.
        if let Ok(msg) = r {
            app.session.echo(msg);
            *open = false;
        }
    }
    if cancel || !*open {
        *open = false;
        ctx.data_mut(|d| d.remove::<PublishState>(id));
    } else {
        ctx.data_mut(|d| d.insert_temp(id, st));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Services;
    use cadcraft_engine::Session;

    #[test]
    fn mledit_opens_its_tools_and_a_tool_starts_the_command() {
        let mut app = CadApp::new(Session::new(), Services::default());
        app.start("mledit");
        assert_eq!(app.ui.dialog.as_deref(), Some("mledit"));
        // What a tool tile does: the engine's MLEDIT with that tool, asking for the multilines.
        app.session.start("mledit").unwrap();
        app.session.input(cadcraft_engine::Input::Keyword("CT".into())).unwrap();
        let prompt = app.session.current_prompt().unwrap();
        assert!(prompt.message.contains("first mline"), "{}", prompt.message);
        assert_eq!(app.ui.dialog.as_deref(), Some("mledit"), "the dialog closes itself, not the command");
    }

    #[test]
    fn every_mledit_tool_is_a_keyword_of_the_command() {
        for (kw, _) in ML_TOOLS.iter().flatten() {
            let mut s = Session::new();
            s.start("mledit").unwrap();
            assert!(s.input(cadcraft_engine::Input::Keyword((*kw).into())).is_ok(), "{kw}");
        }
    }
}
