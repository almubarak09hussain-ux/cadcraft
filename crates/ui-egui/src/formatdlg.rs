//! Format menu dialogs: Select Color (COLOR), Lineweight Settings (LWEIGHT), Current Plot Style
//! (PLOTSTYLE), Edit Drawing Scales (SCALELISTEDIT), the Dimension Style Manager (DIMSTYLE) and
//! Multiline Style (MLSTYLE).
//!
//! Typed or chosen from a menu (no parameters) these commands open their dialog; JSON calls run
//! the command. The dialogs read the drawing and change it only through the same commands
//! (`app.run`), so every change is undoable and scriptable.

use cadcraft_color::{Color, Rgb};
use egui::{RichText, vec2};
use serde_json::{Map, Value, json};

use crate::CadApp;
use crate::theme::Tokens;

/// Open the dialog for a typed or menu-invoked command. `None` if `id` has no dialog here or the
/// call carries parameters.
pub fn route(app: &mut CadApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if !params.is_null() {
        return None;
    }
    let dialog = match cadcraft_engine::cmd::resolve_alias(id).as_str() {
        "color" | "ddcolor" => "color",
        "lweight" => "lweight",
        "plotstyle" => "plotstyle",
        "scalelistedit" => "scalelist",
        "dimstyle" => "dimstyle",
        "mlstyle" => "mlstyle",
        _ => return None,
    };
    if let Err(e) = app.session.doc() {
        return Some(Err(e.to_string()));
    }
    app.ui.dialog = Some(dialog.into());
    Some(Ok(Value::Null))
}

/// Show the dialog named `name`. `false` if it isn't one of these.
pub fn dialog(app: &mut CadApp, ctx: &egui::Context, name: &str, open: &mut bool) -> bool {
    let action = match name {
        "color" => color(app, ctx, open),
        "lweight" => lweight(app, ctx, open),
        "plotstyle" => plot_style(app, ctx, open),
        "scalelist" => scale_list(app, ctx, open),
        "dimstyle" => dim_style(app, ctx, open),
        "mlstyle" => ml_style(app, ctx, open),
        _ => return false,
    };
    for (cmd, p) in action {
        // Errors are echoed on the command line by `run`.
        let _ = app.run(cmd, p);
    }
    true
}

/// Commands to run after the dialog is drawn, in order.
type Actions = Vec<(&'static str, Value)>;

fn temp<T: Clone + Send + Sync + 'static>(ctx: &egui::Context, id: &str) -> Option<T> {
    ctx.data_mut(|d| d.get_temp::<T>(egui::Id::new(id)))
}

fn set_temp<T: Clone + Send + Sync + 'static>(ctx: &egui::Context, id: &str, v: T) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(id), v));
}

fn forget<T: Clone + Send + Sync + 'static>(ctx: &egui::Context, id: &str) {
    ctx.data_mut(|d| d.remove::<T>(egui::Id::new(id)));
}

/// A command's reply, or `Null` when it fails (the dialog then shows what it can).
fn query(app: &mut CadApp, cmd: &str, p: Value) -> Value {
    app.session.execute(cmd, &p).unwrap_or(Value::Null)
}

/// A colour as the commands take it: "ByLayer", "ByBlock", an index or "r,g,b".
pub fn color_param(c: Color) -> Value {
    match c {
        Color::ByLayer => json!("ByLayer"),
        Color::ByBlock => json!("ByBlock"),
        Color::Index(i) => json!(i),
        Color::True(Rgb(r, g, b)) => json!(format!("{r},{g},{b}")),
    }
}

/// A colour as the commands report it (`{kind, value}`, a name or an index).
fn color_from_json(v: &Value) -> Color {
    if let Some(i) = v.as_i64() {
        return Color::from_aci(i16::try_from(i).unwrap_or(256));
    }
    if let Some(s) = v.as_str() {
        return Color::parse(s).unwrap_or(Color::ByLayer);
    }
    match v.get("kind").and_then(Value::as_str) {
        Some("byBlock") => Color::ByBlock,
        Some("index") => v.get("value").and_then(Value::as_u64).and_then(|i| u8::try_from(i).ok()).map_or(Color::ByLayer, Color::Index),
        Some("rgb" | "true" | "trueColor") => {
            let c = |k: &str| v.get(k).and_then(Value::as_u64).and_then(|x| u8::try_from(x).ok()).unwrap_or(0);
            match v.get("value").and_then(Value::as_u64) {
                Some(rgb) => Color::True(Rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)),
                None => Color::True(Rgb(c("r"), c("g"), c("b"))),
            }
        }
        _ => Color::ByLayer,
    }
}

/// An index colour as a DIM*/ACI number (0 ByBlock, 256 ByLayer).
fn aci_number(c: Color) -> i64 {
    match c {
        Color::ByBlock => 0,
        Color::ByLayer => 256,
        Color::Index(i) => i64::from(i),
        Color::True(rgb) => i64::from(cadcraft_color::nearest_aci(rgb)),
    }
}

fn ok_cancel(ui: &mut egui::Ui) -> (bool, bool) {
    ui.separator();
    let mut r = (false, false);
    ui.horizontal(|ui| {
        r.0 = ui.button(crate::tl!("OK")).clicked();
        r.1 = ui.button(crate::tl!("Cancel")).clicked();
    });
    r
}

// ---------- Select Color ----------

/// The current colour for new objects (CECOLOR, with its true colour).
fn current_color(app: &CadApp) -> Color {
    let Ok(d) = app.session.doc() else { return Color::ByLayer };
    let rgb = d.header.i64("CECOLOR_RGB", -1);
    if rgb >= 0 {
        return Color::True(Rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8));
    }
    Color::from_aci(i16::try_from(d.header.i64("CECOLOR", 256)).unwrap_or(256))
}

fn color(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Actions {
    let key = "fmt_color";
    let mut c = temp::<ColorPick>(ctx, key).map_or_else(|| current_color(app), |p| p.0);
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new(crate::tl!("Select Color")).id(egui::Id::new("Select Color")).open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        if let Some(picked) = crate::layers::color_palette(ui, egui::Id::new("fmt_color_palette"), c, true) {
            c = picked;
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(crate::tl!("Color:"));
            let (r, _) = ui.allocate_exact_size(vec2(36.0, 16.0), egui::Sense::hover());
            crate::layers::swatch(ui.painter(), r, c);
            ui.label(crate::i18n::t(&c.name()));
        });
        (ok, cancel) = ok_cancel(ui);
    });
    set_temp(ctx, key, ColorPick(c));
    let mut actions = Vec::new();
    if ok {
        actions.push(("color", json!({ "color": color_param(c) })));
    }
    if ok || cancel || !*open {
        *open = false;
        forget::<ColorPick>(ctx, key);
    }
    actions
}

#[derive(Clone, Copy)]
struct ColorPick(Color);

// ---------- Lineweight Settings ----------

fn lw_label(code: i64) -> String {
    match code {
        -1 => crate::tl!("ByLayer").to_string(),
        -2 => crate::tl!("ByBlock").to_string(),
        -3 => crate::tl!("Default").to_string(),
        n => format!("{:.2} mm", n as f64 / 100.0),
    }
}

fn lweight(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Actions {
    let t = Tokens::get();
    let key = "fmt_lweight";
    let (cur, default_lw) = app.session.doc().map_or((-1, 25), |d| (d.header.i64("CELWEIGHT", -1), d.header.i64("LWDEFAULT", 25)));
    let (mut code, mut display) = temp::<(i64, bool)>(ctx, key).unwrap_or((cur, app.session.settings.lwdisplay));
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new(crate::tl!("Lineweight Settings"))
        .id(egui::Id::new("Lineweight Settings"))
        .open(open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(RichText::new(crate::tl!("Lineweights")).strong());
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for c in [-1i64, -2, -3].into_iter().chain(cadcraft_engine::doc::Lineweight::STANDARD.iter().map(|v| i64::from(*v))) {
                    ui.horizontal(|ui| {
                        // A bar as thick as the lineweight (Default shows LWDEFAULT).
                        let mm = match c {
                            -3 => default_lw as f32 / 100.0,
                            n if n >= 0 => n as f32 / 100.0,
                            _ => 0.25,
                        };
                        let (r, _) = ui.allocate_exact_size(vec2(60.0, 14.0), egui::Sense::hover());
                        ui.painter().line_segment([r.left_center(), r.right_center()], egui::Stroke::new((mm * 4.0).clamp(1.0, 9.0), t.text));
                        if ui.selectable_label(code == c, lw_label(c)).clicked() {
                            code = c;
                        }
                    });
                }
            });
            ui.separator();
            ui.checkbox(&mut display, crate::tl!("Display Lineweight"));
            ui.label(RichText::new(crate::tf!("Default: {lineweight}", lineweight = lw_label(default_lw))).small().color(t.text_dim));
            ui.label(RichText::new(crate::tf!("Current lineweight: {lineweight}", lineweight = lw_label(code))).small().color(t.text_dim));
            (ok, cancel) = ok_cancel(ui);
        });
    set_temp(ctx, key, (code, display));
    let mut actions = Vec::new();
    if ok {
        let lw = match code {
            -1 => json!("ByLayer"),
            -2 => json!("ByBlock"),
            -3 => json!("Default"),
            n => json!(n as f64 / 100.0),
        };
        actions.push(("lweight", json!({ "lineweight": lw })));
        if display != app.session.settings.lwdisplay {
            actions.push(("setvar", json!({ "name": "LWDISPLAY", "value": i64::from(display) })));
        }
    }
    if ok || cancel || !*open {
        *open = false;
        forget::<(i64, bool)>(ctx, key);
    }
    actions
}

// ---------- Current Plot Style ----------

fn plot_style(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Actions {
    let t = Tokens::get();
    let key = "fmt_plotstyle";
    let info = query(app, "plotstyle", json!({}));
    let current = info["current"].as_str().unwrap_or("ByLayer").to_string();
    let named: Vec<String> = info["styles"].as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let table = app.session.doc().map(|d| d.header.str("STYLESHEET", "")).unwrap_or_default();
    let mut pick = temp::<String>(ctx, key).unwrap_or_else(|| current.clone());
    let (mut ok, mut cancel, mut editor) = (false, false, false);
    egui::Window::new(crate::tl!("Current Plot Style")).id(egui::Id::new("Current Plot Style")).open(open).collapsible(false).resizable(false).show(
        ctx,
        |ui| {
            ui.label(RichText::new(crate::tl!("Plot styles")).strong());
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                for s in ["ByLayer".to_string(), "ByBlock".to_string()].iter().chain(named.iter()) {
                    if ui.selectable_label(pick.eq_ignore_ascii_case(s), crate::i18n::t(s)).clicked() {
                        pick = s.clone();
                    }
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(crate::tl!("Active plot style table:"));
                ui.label(if table.is_empty() { crate::tl!("(none)").to_string() } else { table.clone() });
            });
            if ui.button(crate::tl!("Plot Style Table Editor...")).clicked() {
                editor = true;
            }
            ui.label(RichText::new(crate::tf!("Current plot style: {style}", style = current)).small().color(t.text_dim));
            (ok, cancel) = ok_cancel(ui);
        },
    );
    set_temp(ctx, key, pick.clone());
    let mut actions = Vec::new();
    if ok && !pick.eq_ignore_ascii_case(&current) {
        // With nothing selected PLOTSTYLE sets the style for new objects (CPLOTSTYLE).
        app.session.set_selection(Vec::new());
        actions.push(("plotstyle", json!({ "name": pick })));
    }
    if ok || cancel || editor || !*open {
        *open = false;
        forget::<String>(ctx, key);
    }
    if editor {
        app.ui.dialog = Some("stylesmanager".into());
    }
    actions
}

// ---------- Edit Drawing Scales ----------

#[derive(Clone, Default)]
struct ScaleForm {
    sel: Option<String>,
    name: String,
    paper: String,
    drawing: String,
}

fn scale_list(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Actions {
    let t = Tokens::get();
    let key = "fmt_scalelist";
    let info = query(app, "scalelistedit", json!({}));
    let current = info["current"].as_str().unwrap_or_default().to_string();
    let scales: Vec<(String, f64, f64)> = info["scales"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| {
                    Some((s["name"].as_str()?.to_string(), s["paperUnits"].as_f64().unwrap_or(1.0), s["drawingUnits"].as_f64().unwrap_or(1.0)))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut f = temp::<ScaleForm>(ctx, key).unwrap_or_else(|| ScaleForm { paper: "1".into(), drawing: "1".into(), ..ScaleForm::default() });
    let mut actions = Vec::new();
    let (mut ok, mut close) = (false, false);
    egui::Window::new(crate::tl!("Edit Drawing Scales"))
        .id(egui::Id::new("Edit Drawing Scales"))
        .open(open)
        .collapsible(false)
        .default_size(vec2(440.0, 380.0))
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(crate::tl!("Scale List")).strong());
                    egui::ScrollArea::vertical().max_height(260.0).min_scrolled_width(180.0).show(ui, |ui| {
                        for (name, p, dr) in &scales {
                            let label = if *name == current { RichText::new(name).strong() } else { RichText::new(name) };
                            if ui.selectable_label(f.sel.as_deref() == Some(name.as_str()), label).on_hover_text(format!("{p} = {dr}")).clicked() {
                                f.sel = Some(name.clone());
                            }
                        }
                    });
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.label(RichText::new(crate::tl!("Add a scale")).strong());
                    egui::Grid::new("scalelist_add").num_columns(2).show(ui, |ui| {
                        ui.label(crate::tl!("Name"));
                        ui.add(egui::TextEdit::singleline(&mut f.name).desired_width(110.0).hint_text("1:25"));
                        ui.end_row();
                        ui.label(crate::tl!("Paper units"));
                        ui.add(egui::TextEdit::singleline(&mut f.paper).desired_width(110.0));
                        ui.end_row();
                        ui.label(crate::tl!("Drawing units"));
                        ui.add(egui::TextEdit::singleline(&mut f.drawing).desired_width(110.0));
                        ui.end_row();
                    });
                    let (p, d) = (f.paper.trim().parse::<f64>().ok(), f.drawing.trim().parse::<f64>().ok());
                    let valid =
                        !f.name.trim().is_empty() && p.is_some_and(|v| v > 0.0 && v.is_finite()) && d.is_some_and(|v| v > 0.0 && v.is_finite());
                    if ui.add_enabled(valid, egui::Button::new(crate::tl!("Add"))).clicked()
                        && let (Some(p), Some(d)) = (p, d)
                    {
                        actions.push(("scalelistedit", json!({ "add": { "name": f.name.trim(), "paperUnits": p, "drawingUnits": d } })));
                        f.name.clear();
                    }
                    ui.add_space(8.0);
                    if ui.add_enabled(f.sel.is_some(), egui::Button::new(crate::tl!("Delete"))).clicked()
                        && let Some(n) = f.sel.take()
                    {
                        actions.push(("scalelistedit", json!({ "delete": n })));
                    }
                    ui.horizontal(|ui| {
                        if ui.button(crate::tl!("Reset (metric)")).clicked() {
                            actions.push(("scalelistedit", json!({ "reset": "metric" })));
                        }
                        if ui.button(crate::tl!("Reset (imperial)")).clicked() {
                            actions.push(("scalelistedit", json!({ "reset": "imperial" })));
                        }
                    });
                    ui.label(RichText::new(crate::tl!("Scales in use by viewports can't be deleted.")).small().color(t.text_dim));
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                ok = ui.button(crate::tl!("OK")).clicked();
                close = ui.button(crate::tl!("Close")).clicked();
            });
        });
    set_temp(ctx, key, f);
    if ok || close || !*open {
        *open = false;
        forget::<ScaleForm>(ctx, key);
    }
    actions
}

// ---------- Dimension Style Manager ----------

/// How a dimension variable is edited.
#[derive(Clone, Copy)]
enum Kind {
    Num,
    Int(&'static [(i64, &'static str)]),
    Bool,
    Color,
    Text,
}

const DIMTAD: &[(i64, &str)] = &[(0, "Centered"), (1, "Above"), (2, "Outside"), (3, "JIS"), (4, "Below")];
const DIMJUST: &[(i64, &str)] = &[(0, "Centered"), (1, "At Ext Line 1"), (2, "At Ext Line 2"), (3, "Over Ext Line 1"), (4, "Over Ext Line 2")];
const DIMATFIT: &[(i64, &str)] = &[(0, "Both text and arrows outside"), (1, "Arrows first"), (2, "Text first"), (3, "Best fit")];
const DIMLUNIT: &[(i64, &str)] =
    &[(1, "Scientific"), (2, "Decimal"), (3, "Engineering"), (4, "Architectural"), (5, "Fractional"), (6, "Windows Desktop")];
const DIMDEC: &[(i64, &str)] =
    &[(0, "0"), (1, "0.0"), (2, "0.00"), (3, "0.000"), (4, "0.0000"), (5, "0.00000"), (6, "0.000000"), (7, "0.0000000"), (8, "0.00000000")];
const DIMAUNIT: &[(i64, &str)] = &[(0, "Decimal Degrees"), (1, "Degrees Minutes Seconds"), (2, "Gradians"), (3, "Radians")];
const DIMTMOVE: &[(i64, &str)] =
    &[(0, "Beside the dimension line"), (1, "Over dimension line, with leader"), (2, "Over dimension line, without leader")];

/// The Modify dialog's tabs: (tab, [(variable, label, kind)]).
const DIM_TABS: &[(&str, &[(&str, &str, Kind)])] = &[
    (
        "Lines",
        &[
            ("DIMCLRD", "Dimension line color", Kind::Color),
            ("DIMDLE", "Extend beyond ticks", Kind::Num),
            ("DIMDLI", "Baseline spacing", Kind::Num),
            ("DIMSD1", "Suppress dim line 1", Kind::Bool),
            ("DIMSD2", "Suppress dim line 2", Kind::Bool),
            ("DIMCLRE", "Extension line color", Kind::Color),
            ("DIMEXE", "Extend beyond dim lines", Kind::Num),
            ("DIMEXO", "Offset from origin", Kind::Num),
            ("DIMSE1", "Suppress ext line 1", Kind::Bool),
            ("DIMSE2", "Suppress ext line 2", Kind::Bool),
            ("DIMFXLON", "Fixed length extension lines", Kind::Bool),
            ("DIMFXL", "Extension line length", Kind::Num),
        ],
    ),
    (
        "Symbols and Arrows",
        &[
            ("DIMBLK", "Arrowheads (both)", Kind::Text),
            ("DIMBLK1", "First arrowhead", Kind::Text),
            ("DIMBLK2", "Second arrowhead", Kind::Text),
            ("DIMLDRBLK", "Leader arrowhead", Kind::Text),
            ("DIMASZ", "Arrow size", Kind::Num),
            ("DIMTSZ", "Tick size (0: arrows)", Kind::Num),
            ("DIMCEN", "Center mark size (negative: lines)", Kind::Num),
            ("DIMJOGANG", "Radius jog angle", Kind::Num),
        ],
    ),
    (
        "Text",
        &[
            ("DIMTXSTY", "Text style", Kind::Text),
            ("DIMCLRT", "Text color", Kind::Color),
            ("DIMTXT", "Text height", Kind::Num),
            ("DIMTAD", "Vertical placement", Kind::Int(DIMTAD)),
            ("DIMJUST", "Horizontal placement", Kind::Int(DIMJUST)),
            ("DIMGAP", "Offset from dim line", Kind::Num),
            ("DIMTIH", "Text inside horizontal", Kind::Bool),
            ("DIMTOH", "Text outside horizontal", Kind::Bool),
        ],
    ),
    (
        "Fit",
        &[
            ("DIMATFIT", "Fit options", Kind::Int(DIMATFIT)),
            ("DIMTIX", "Always keep text between ext lines", Kind::Bool),
            ("DIMSOXD", "Suppress arrows outside ext lines", Kind::Bool),
            ("DIMTMOVE", "Text placement when moved", Kind::Int(DIMTMOVE)),
            ("DIMSCALE", "Overall scale", Kind::Num),
            ("DIMTOFL", "Draw dim line between ext lines", Kind::Bool),
        ],
    ),
    (
        "Primary Units",
        &[
            ("DIMLUNIT", "Unit format", Kind::Int(DIMLUNIT)),
            ("DIMDEC", "Precision", Kind::Int(DIMDEC)),
            ("DIMDSEP", "Decimal separator", Kind::Text),
            ("DIMRND", "Round off", Kind::Num),
            ("DIMPOST", "Prefix/suffix (<> is the value)", Kind::Text),
            ("DIMLFAC", "Measurement scale factor", Kind::Num),
            ("DIMZIN", "Zero suppression (DIMZIN)", Kind::Num),
            ("DIMAUNIT", "Angular unit format", Kind::Int(DIMAUNIT)),
            ("DIMADEC", "Angular precision", Kind::Int(DIMDEC)),
        ],
    ),
    (
        "Alternate Units",
        &[
            ("DIMALT", "Display alternate units", Kind::Bool),
            ("DIMALTU", "Unit format", Kind::Int(DIMLUNIT)),
            ("DIMALTD", "Precision", Kind::Int(DIMDEC)),
            ("DIMALTF", "Multiplier", Kind::Num),
            ("DIMALTRND", "Round off", Kind::Num),
            ("DIMAPOST", "Prefix/suffix", Kind::Text),
        ],
    ),
    (
        "Tolerances",
        &[
            ("DIMTOL", "Show tolerances", Kind::Bool),
            ("DIMLIM", "Show limits", Kind::Bool),
            ("DIMTP", "Upper value", Kind::Num),
            ("DIMTM", "Lower value", Kind::Num),
            ("DIMTDEC", "Precision", Kind::Int(DIMDEC)),
            ("DIMTFAC", "Scaling for height", Kind::Num),
        ],
    ),
];

/// The Dimension Style Manager's state while it is open.
#[derive(Clone, Default)]
struct DimState {
    sel: String,
    /// Modify/New: the style edited and its variables (edited, not yet applied).
    editing: Option<(String, Map<String, Value>)>,
    tab: usize,
    /// New Style: name and the style to start from.
    new_name: Option<(String, String)>,
    rename: Option<String>,
}

/// A style's variables as the commands take them (colours as ACI numbers).
fn dim_vars(app: &mut CadApp, name: &str) -> Map<String, Value> {
    let r = query(app, "dimstyle.list", json!({ "name": name }));
    let mut vars = r["variables"].as_object().cloned().unwrap_or_default();
    for (_, v) in vars.iter_mut() {
        if v.is_object() {
            *v = json!(aci_number(color_from_json(v)));
        }
    }
    vars
}

fn dim_style(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Actions {
    let t = Tokens::get();
    let key = "fmt_dimstyle";
    let info = query(app, "dimstyle.list", json!({}));
    let current = info["current"].as_str().unwrap_or("Standard").to_string();
    let styles: Vec<String> =
        info["styles"].as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let mut st = temp::<DimState>(ctx, key).unwrap_or_else(|| DimState { sel: current.clone(), ..DimState::default() });
    if !styles.iter().any(|s| s.eq_ignore_ascii_case(&st.sel)) {
        st.sel = current.clone();
    }
    let mut actions = Vec::new();
    let mut close = false;
    let mut start_edit: Option<String> = None;
    egui::Window::new(crate::tl!("Dimension Style Manager"))
        .id(egui::Id::new("Dimension Style Manager"))
        .open(open)
        .collapsible(false)
        .default_size(vec2(460.0, 320.0))
        .show(ctx, |ui| {
            ui.label(RichText::new(crate::tf!("Current dimstyle: {style}", style = current)).color(t.text_dim));
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(crate::tl!("Styles")).strong());
                    egui::ScrollArea::vertical().max_height(220.0).min_scrolled_width(200.0).show(ui, |ui| {
                        for s in &styles {
                            // The current style in bold.
                            let label = if *s == current { RichText::new(s).strong() } else { RichText::new(s) };
                            let r = ui.selectable_label(st.sel == *s, label);
                            if r.clicked() {
                                st.sel = s.clone();
                            }
                            if r.double_clicked() {
                                start_edit = Some(s.clone());
                            }
                        }
                    });
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_min_width(140.0);
                    if ui.add_enabled(st.sel != current, egui::Button::new(crate::tl!("Set Current"))).clicked() {
                        actions.push(("dimstyle.current", json!({ "name": st.sel })));
                    }
                    if ui.button(crate::tl!("New...")).clicked() {
                        st.new_name = Some((format!("Copy of {}", st.sel), st.sel.clone()));
                    }
                    if ui.button(crate::tl!("Modify...")).clicked() {
                        start_edit = Some(st.sel.clone());
                    }
                    if ui.add_enabled(!st.sel.eq_ignore_ascii_case("Standard"), egui::Button::new(crate::tl!("Rename..."))).clicked() {
                        st.rename = Some(st.sel.clone());
                    }
                    if ui
                        .add_enabled(st.sel != current && !st.sel.eq_ignore_ascii_case("Standard"), egui::Button::new(crate::tl!("Delete")))
                        .clicked()
                    {
                        actions.push(("dimstyle.delete", json!({ "name": st.sel })));
                    }
                });
            });
            ui.separator();
            close = ui.button(crate::tl!("Close")).clicked();
        });
    if let Some(name) = start_edit {
        let vars = dim_vars(app, &name);
        st.editing = Some((name, vars));
    }
    // Create New Dimension Style.
    if let Some((mut name, mut from)) = st.new_name.take() {
        let (mut go, mut cancel) = (false, false);
        egui::Window::new(crate::tl!("Create New Dimension Style"))
            .id(egui::Id::new("Create New Dimension Style"))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("dim_new").num_columns(2).show(ui, |ui| {
                    ui.label(crate::tl!("New Style Name:"));
                    ui.text_edit_singleline(&mut name);
                    ui.end_row();
                    ui.label(crate::tl!("Start With:"));
                    egui::ComboBox::from_id_salt("dim_new_from").selected_text(&from).show_ui(ui, |ui| {
                        for s in &styles {
                            ui.selectable_value(&mut from, s.clone(), s);
                        }
                    });
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    go = ui
                        .add_enabled(
                            !name.trim().is_empty() && !styles.iter().any(|s| s.eq_ignore_ascii_case(name.trim())),
                            egui::Button::new(crate::tl!("Continue")),
                        )
                        .clicked();
                    cancel = ui.button(crate::tl!("Cancel")).clicked();
                });
            });
        if go {
            // The new style starts as a copy; Modify opens on it to change what differs.
            let mut vars = dim_vars(app, &from);
            vars.insert("name".into(), json!(name.trim()));
            vars.insert("current".into(), json!(false));
            actions.push(("dimstyle", Value::Object(vars.clone())));
            vars.remove("name");
            vars.remove("current");
            st.sel = name.trim().to_string();
            st.editing = Some((st.sel.clone(), vars));
        } else if !cancel {
            st.new_name = Some((name, from));
        }
    }
    // Rename.
    if let Some(mut to) = st.rename.take() {
        let (mut go, mut cancel) = (false, false);
        egui::Window::new(crate::tl!("Rename Dimension Style")).id(egui::Id::new("Rename Dimension Style")).collapsible(false).resizable(false).show(
            ctx,
            |ui| {
                ui.text_edit_singleline(&mut to);
                ui.horizontal(|ui| {
                    go = ui.add_enabled(!to.trim().is_empty(), egui::Button::new(crate::tl!("OK"))).clicked();
                    cancel = ui.button(crate::tl!("Cancel")).clicked();
                });
            },
        );
        if go {
            actions.push(("dimstyle.rename", json!({ "from": st.sel, "to": to.trim() })));
            st.sel = to.trim().to_string();
        } else if !cancel {
            st.rename = Some(to);
        }
    }
    // Modify Dimension Style.
    if let Some((name, mut vars)) = st.editing.take() {
        let (mut ok, mut cancel) = (false, false);
        let mut win_open = true;
        egui::Window::new(crate::tf!("Modify Dimension Style: {style}", style = name))
            .id(egui::Id::new("Modify Dimension Style"))
            .open(&mut win_open)
            .collapsible(false)
            .default_size(vec2(520.0, 420.0))
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (i, (tab, _)) in DIM_TABS.iter().enumerate() {
                        if ui.selectable_label(st.tab == i, crate::i18n::t(tab)).clicked() {
                            st.tab = i;
                        }
                    }
                });
                ui.separator();
                let fields = DIM_TABS.get(st.tab).map_or(&[][..], |(_, f)| *f);
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    egui::Grid::new("dim_fields").num_columns(2).spacing(vec2(16.0, 6.0)).show(ui, |ui| {
                        for (var, label, kind) in fields {
                            ui.label(crate::i18n::t(label)).on_hover_text(*var);
                            dim_field(ui, &mut vars, var, *kind);
                            ui.end_row();
                        }
                    });
                });
                (ok, cancel) = ok_cancel(ui);
            });
        if ok {
            let mut p = vars.clone();
            p.insert("name".into(), json!(name));
            p.insert("current".into(), json!(name == current));
            actions.push(("dimstyle", Value::Object(p)));
        } else if !cancel && win_open {
            st.editing = Some((name, vars));
        }
    }
    set_temp(ctx, key, st);
    if close || !*open {
        *open = false;
        forget::<DimState>(ctx, key);
    }
    actions
}

fn dim_field(ui: &mut egui::Ui, vars: &mut Map<String, Value>, var: &str, kind: Kind) {
    let v = vars.get(var).cloned().unwrap_or(Value::Null);
    let new = match kind {
        Kind::Num => {
            let mut x = v.as_f64().unwrap_or(0.0);
            ui.add(egui::DragValue::new(&mut x).speed(0.01).min_decimals(4).max_decimals(8)).changed().then(|| json!(x))
        }
        Kind::Int(choices) => {
            let mut x = v.as_i64().unwrap_or(0);
            let shown = choices.iter().find(|(c, _)| *c == x).map_or_else(|| x.to_string(), |(_, l)| crate::i18n::t(l).to_string());
            let mut changed = false;
            egui::ComboBox::from_id_salt(("dimvar", var)).selected_text(shown).width(220.0).show_ui(ui, |ui| {
                for (c, l) in choices {
                    changed |= ui.selectable_value(&mut x, *c, crate::i18n::t(l)).changed();
                }
            });
            changed.then(|| json!(x))
        }
        Kind::Bool => {
            let mut b = v.as_bool().unwrap_or(false);
            ui.checkbox(&mut b, "").changed().then(|| json!(b))
        }
        Kind::Color => {
            let c = color_from_json(&v);
            crate::layers::color_button(ui, egui::Id::new(("dimclr", var)), c, true, 140.0).map(|c| json!(aci_number(c)))
        }
        Kind::Text => {
            let mut s = v.as_str().unwrap_or_default().to_string();
            ui.add(egui::TextEdit::singleline(&mut s).desired_width(160.0)).changed().then(|| json!(s))
        }
    };
    if let Some(n) = new {
        vars.insert(var.to_string(), n);
    }
}

// ---------- Multiline Style ----------

/// The Multiline Style dialog's state while it is open.
#[derive(Clone, Default)]
struct MlState {
    sel: String,
    /// Modify/New: the style's name, whether it is new, and its definition being edited.
    editing: Option<(String, bool, Value)>,
    new_name: Option<(String, String)>,
    rename: Option<String>,
    /// The element selected in the Modify dialog.
    element: usize,
}

fn ml_style(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Actions {
    let t = Tokens::get();
    let key = "fmt_mlstyle";
    let info = query(app, "mlstyle", json!({}));
    let current = info["current"].as_str().unwrap_or("STANDARD").to_string();
    let styles: Vec<Value> = info["styles"].as_array().cloned().unwrap_or_default();
    let names: Vec<String> = styles.iter().filter_map(|s| s["name"].as_str().map(str::to_string)).collect();
    let mut st = temp::<MlState>(ctx, key).unwrap_or_else(|| MlState { sel: current.clone(), ..MlState::default() });
    if !names.iter().any(|s| s.eq_ignore_ascii_case(&st.sel)) {
        st.sel = current.clone();
    }
    let style_of = |n: &str| styles.iter().find(|s| s["name"].as_str().is_some_and(|x| x.eq_ignore_ascii_case(n))).cloned();
    let mut actions = Vec::new();
    let mut close = false;
    let mut start_edit = None;
    egui::Window::new(crate::tl!("Multiline Style"))
        .id(egui::Id::new("Multiline Style"))
        .open(open)
        .collapsible(false)
        .default_size(vec2(460.0, 340.0))
        .show(ctx, |ui| {
            ui.label(RichText::new(crate::tf!("Current multiline style: {style}", style = current)).color(t.text_dim));
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(crate::tl!("Styles")).strong());
                    egui::ScrollArea::vertical().max_height(180.0).min_scrolled_width(200.0).show(ui, |ui| {
                        for s in &names {
                            let label = if s.eq_ignore_ascii_case(&current) { RichText::new(s).strong() } else { RichText::new(s) };
                            let r = ui.selectable_label(st.sel.eq_ignore_ascii_case(s), label);
                            if r.clicked() {
                                st.sel = s.clone();
                            }
                            if r.double_clicked() {
                                start_edit = Some(s.clone());
                            }
                        }
                    });
                    // A preview of the selected style's elements.
                    if let Some(s) = style_of(&st.sel) {
                        ml_preview(ui, &s);
                        let desc = s["description"].as_str().unwrap_or_default();
                        if !desc.is_empty() {
                            ui.label(RichText::new(desc).small().color(t.text_dim));
                        }
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_min_width(140.0);
                    if ui.add_enabled(!st.sel.eq_ignore_ascii_case(&current), egui::Button::new(crate::tl!("Set Current"))).clicked()
                        && let Some(mut s) = style_of(&st.sel)
                    {
                        // MLSTYLE with a style's own definition makes it current (no change otherwise).
                        s["current"] = json!(true);
                        actions.push(("mlstyle", ml_params(&s)));
                    }
                    if ui.button(crate::tl!("New...")).clicked() {
                        st.new_name = Some((String::new(), st.sel.clone()));
                    }
                    if ui.button(crate::tl!("Modify...")).clicked() {
                        start_edit = Some(st.sel.clone());
                    }
                    if ui.add_enabled(!st.sel.eq_ignore_ascii_case("STANDARD"), egui::Button::new(crate::tl!("Rename..."))).clicked() {
                        st.rename = Some(st.sel.clone());
                    }
                    if ui
                        .add_enabled(
                            !st.sel.eq_ignore_ascii_case("STANDARD") && !st.sel.eq_ignore_ascii_case(&current),
                            egui::Button::new(crate::tl!("Delete")),
                        )
                        .clicked()
                    {
                        actions.push(("mlstyle", json!({ "delete": st.sel })));
                    }
                });
            });
            ui.separator();
            close = ui.button(crate::tl!("Close")).clicked();
        });
    if let Some(n) = start_edit
        && let Some(s) = style_of(&n)
    {
        st.editing = Some((n, false, s));
        st.element = 0;
    }
    if let Some((mut name, mut from)) = st.new_name.take() {
        let (mut go, mut cancel) = (false, false);
        egui::Window::new(crate::tl!("Create New Multiline Style"))
            .id(egui::Id::new("Create New Multiline Style"))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("ml_new").num_columns(2).show(ui, |ui| {
                    ui.label(crate::tl!("New Style Name:"));
                    ui.text_edit_singleline(&mut name);
                    ui.end_row();
                    ui.label(crate::tl!("Start With:"));
                    egui::ComboBox::from_id_salt("ml_new_from").selected_text(&from).show_ui(ui, |ui| {
                        for s in &names {
                            ui.selectable_value(&mut from, s.clone(), s);
                        }
                    });
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    go = ui
                        .add_enabled(
                            !name.trim().is_empty() && !names.iter().any(|s| s.eq_ignore_ascii_case(name.trim())),
                            egui::Button::new(crate::tl!("Continue")),
                        )
                        .clicked();
                    cancel = ui.button(crate::tl!("Cancel")).clicked();
                });
            });
        if go && let Some(mut s) = style_of(&from) {
            s["name"] = json!(name.trim().to_ascii_uppercase());
            st.editing = Some((name.trim().to_ascii_uppercase(), true, s));
            st.element = 0;
        } else if !go && !cancel {
            st.new_name = Some((name, from));
        }
    }
    if let Some(mut to) = st.rename.take() {
        let (mut go, mut cancel) = (false, false);
        egui::Window::new(crate::tl!("Rename Multiline Style")).id(egui::Id::new("Rename Multiline Style")).collapsible(false).resizable(false).show(
            ctx,
            |ui| {
                ui.text_edit_singleline(&mut to);
                ui.horizontal(|ui| {
                    go = ui.add_enabled(!to.trim().is_empty(), egui::Button::new(crate::tl!("OK"))).clicked();
                    cancel = ui.button(crate::tl!("Cancel")).clicked();
                });
            },
        );
        if go {
            actions.push(("mlstyle", json!({ "name": st.sel, "rename": to.trim() })));
            st.sel = to.trim().to_ascii_uppercase();
        } else if !cancel {
            st.rename = Some(to);
        }
    }
    if let Some((name, is_new, mut s)) = st.editing.take() {
        let (mut ok, mut cancel) = (false, false);
        let mut win_open = true;
        let title = if is_new {
            crate::tf!("New Multiline Style: {style}", style = name)
        } else {
            crate::tf!("Modify Multiline Style: {style}", style = name)
        };
        egui::Window::new(title)
            .id(egui::Id::new("Modify Multiline Style"))
            .open(&mut win_open)
            .collapsible(false)
            .default_size(vec2(560.0, 420.0))
            .show(ctx, |ui| {
                ml_editor(ui, &mut s, &mut st.element);
                (ok, cancel) = ok_cancel(ui);
            });
        if ok {
            let mut p = ml_params(&s);
            p["name"] = json!(name);
            p["current"] = json!(is_new || name.eq_ignore_ascii_case(&current));
            actions.push(("mlstyle", p));
            st.sel = name;
        } else if !cancel && win_open {
            st.editing = Some((name, is_new, s));
        }
    }
    set_temp(ctx, key, st);
    if close || !*open {
        *open = false;
        forget::<MlState>(ctx, key);
    }
    actions
}

/// A style as MLSTYLE reports it, turned into MLSTYLE's parameters (angles in degrees, colours
/// as the command takes them).
fn ml_params(s: &Value) -> Value {
    let caps = |c: &Value| {
        json!({
            "line": c["line"].as_bool().unwrap_or(false),
            "outerArc": c["outerArc"].as_bool().unwrap_or(false),
            "innerArcs": c["innerArcs"].as_bool().unwrap_or(false),
            "angle": c["angle"].as_f64().unwrap_or(std::f64::consts::FRAC_PI_2).to_degrees(),
        })
    };
    let elements: Vec<Value> = s["elements"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|e| json!({ "offset": e["offset"].as_f64().unwrap_or(0.0), "color": color_param(color_from_json(&e["color"])), "linetype": e["linetype"].as_str().unwrap_or("ByLayer") }))
                .collect()
        })
        .unwrap_or_default();
    json!({
        "name": s["name"],
        "description": s["description"].as_str().unwrap_or_default(),
        "elements": elements,
        "fill": if s["fill"].is_null() || s["fill"] == json!(false) { json!(false) } else { color_param(color_from_json(&s["fill"])) },
        "joints": s["joints"].as_bool().unwrap_or(false),
        "startCaps": caps(&s["startCaps"]),
        "endCaps": caps(&s["endCaps"]),
        "current": s["current"].as_bool().unwrap_or(false),
    })
}

/// The elements as parallel lines, to scale, with their colours.
fn ml_preview(ui: &mut egui::Ui, s: &Value) {
    let (r, _) = ui.allocate_exact_size(vec2(200.0, 60.0), egui::Sense::hover());
    let t = Tokens::get();
    ui.painter().rect_filled(r, 2.0, t.canvas);
    let els: Vec<(f64, Color)> = s["elements"]
        .as_array()
        .map(|a| a.iter().map(|e| (e["offset"].as_f64().unwrap_or(0.0), color_from_json(&e["color"]))).collect())
        .unwrap_or_default();
    let span = els.iter().map(|e| e.0.abs()).fold(0.0f64, f64::max).max(1e-9);
    for (off, c) in els {
        let y = r.center().y - (off / span) as f32 * (r.height() * 0.38);
        let rgb = c.resolve(Color::Index(7), Color::Index(7));
        ui.painter().line_segment(
            [egui::pos2(r.left() + 10.0, y), egui::pos2(r.right() - 10.0, y)],
            egui::Stroke::new(1.5, egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2)),
        );
    }
}

/// The Modify Multiline Style body: description, caps, fill, joints and the elements table.
fn ml_editor(ui: &mut egui::Ui, s: &mut Value, element: &mut usize) {
    let t = Tokens::get();
    let mut desc = s["description"].as_str().unwrap_or_default().to_string();
    ui.horizontal(|ui| {
        ui.label(crate::tl!("Description:"));
        if ui.add(egui::TextEdit::singleline(&mut desc).desired_width(360.0)).changed() {
            s["description"] = json!(desc);
        }
    });
    ui.separator();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(crate::tl!("Caps")).strong());
            egui::Grid::new("ml_caps").num_columns(3).show(ui, |ui| {
                ui.label("");
                ui.label(crate::tl!("Start"));
                ui.label(crate::tl!("End"));
                ui.end_row();
                for (k, label) in [("line", "Line"), ("outerArc", "Outer arc"), ("innerArcs", "Inner arcs")] {
                    ui.label(crate::i18n::t(label));
                    for side in ["startCaps", "endCaps"] {
                        let mut b = s[side][k].as_bool().unwrap_or(false);
                        if ui.checkbox(&mut b, "").changed() {
                            s[side][k] = json!(b);
                        }
                    }
                    ui.end_row();
                }
                ui.label(crate::tl!("Angle"));
                for side in ["startCaps", "endCaps"] {
                    let mut deg = s[side]["angle"].as_f64().unwrap_or(std::f64::consts::FRAC_PI_2).to_degrees();
                    if ui.add(egui::DragValue::new(&mut deg).range(10.0..=170.0).suffix("°")).changed() {
                        s[side]["angle"] = json!(deg.to_radians());
                    }
                }
                ui.end_row();
            });
            ui.add_space(6.0);
            let mut fill_on = !(s["fill"].is_null() || s["fill"] == json!(false));
            ui.horizontal(|ui| {
                if ui.checkbox(&mut fill_on, crate::tl!("Fill color")).changed() {
                    s["fill"] = if fill_on { json!({ "kind": "byLayer" }) } else { Value::Null };
                }
                if fill_on && let Some(c) = crate::layers::color_button(ui, egui::Id::new("ml_fill"), color_from_json(&s["fill"]), true, 120.0) {
                    s["fill"] = color_param(c);
                }
            });
            let mut joints = s["joints"].as_bool().unwrap_or(false);
            if ui.checkbox(&mut joints, crate::tl!("Display joints")).changed() {
                s["joints"] = json!(joints);
            }
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.label(RichText::new(crate::tl!("Elements")).strong());
            let mut els = s["elements"].as_array().cloned().unwrap_or_default();
            egui::Grid::new("ml_els").num_columns(3).striped(true).show(ui, |ui| {
                ui.label(RichText::new(crate::tl!("Offset")).color(t.text_dim));
                ui.label(RichText::new(crate::tl!("Color")).color(t.text_dim));
                ui.label(RichText::new(crate::tl!("Linetype")).color(t.text_dim));
                ui.end_row();
                for (i, e) in els.iter().enumerate() {
                    let off = e["offset"].as_f64().unwrap_or(0.0);
                    if ui.selectable_label(*element == i, format!("{off:.3}")).clicked() {
                        *element = i;
                    }
                    ui.label(crate::i18n::t(&color_from_json(&e["color"]).name()));
                    ui.label(e["linetype"].as_str().unwrap_or("ByLayer"));
                    ui.end_row();
                }
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(els.len() < 16, egui::Button::new(crate::tl!("Add"))).clicked() {
                    els.push(json!({ "offset": 0.0, "color": { "kind": "byLayer" }, "linetype": "ByLayer" }));
                    *element = els.len() - 1;
                }
                if ui.add_enabled(els.len() > 1, egui::Button::new(crate::tl!("Delete"))).clicked() && *element < els.len() {
                    els.remove(*element);
                    *element = element.saturating_sub(1);
                }
            });
            if let Some(e) = els.get_mut(*element) {
                egui::Grid::new("ml_el_edit").num_columns(2).show(ui, |ui| {
                    ui.label(crate::tl!("Offset"));
                    let mut off = e["offset"].as_f64().unwrap_or(0.0);
                    if ui.add(egui::DragValue::new(&mut off).speed(0.01).range(-1e6..=1e6)).changed() {
                        e["offset"] = json!(off);
                    }
                    ui.end_row();
                    ui.label(crate::tl!("Color"));
                    if let Some(c) = crate::layers::color_button(ui, egui::Id::new("ml_el_color"), color_from_json(&e["color"]), true, 120.0) {
                        e["color"] = color_param(c);
                    }
                    ui.end_row();
                    ui.label(crate::tl!("Linetype"));
                    let mut lt = e["linetype"].as_str().unwrap_or("ByLayer").to_string();
                    if ui.add(egui::TextEdit::singleline(&mut lt).desired_width(120.0)).changed() {
                        e["linetype"] = json!(lt);
                    }
                    ui.end_row();
                });
            }
            s["elements"] = Value::Array(els);
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Services;
    use cadcraft_engine::Session;

    #[test]
    fn format_commands_from_the_menu_open_their_dialogs() {
        let mut app = CadApp::new(Session::new(), Services::default());
        for (cmd, dialog) in [
            ("color", "color"),
            ("col", "color"),
            ("lweight", "lweight"),
            ("lw", "lweight"),
            ("plotstyle", "plotstyle"),
            ("scalelistedit", "scalelist"),
            ("dimstyle", "dimstyle"),
            ("d", "dimstyle"),
            ("mlstyle", "mlstyle"),
        ] {
            app.ui.dialog = None;
            app.start(cmd);
            assert_eq!(app.ui.dialog.as_deref(), Some(dialog), "{cmd}");
        }
        // JSON calls still run the commands, without a dialog.
        app.ui.dialog = None;
        app.run("color", json!({ "color": 1 })).unwrap();
        assert!(app.ui.dialog.is_none());
        assert_eq!(current_color(&app), Color::Index(1));
    }

    #[test]
    fn colours_round_trip_through_the_command_parameters() {
        for c in [Color::ByLayer, Color::ByBlock, Color::Index(1), Color::Index(34), Color::True(Rgb(10, 20, 30))] {
            assert_eq!(color_from_json(&color_param(c)), c, "{c:?}");
        }
        assert_eq!(color_from_json(&json!({ "kind": "index", "value": 5 })), Color::Index(5));
        assert_eq!(color_from_json(&json!({ "kind": "byBlock" })), Color::ByBlock);
        assert_eq!(aci_number(Color::ByLayer), 256);
        assert_eq!(aci_number(Color::ByBlock), 0);
    }

    #[test]
    fn a_style_reported_by_mlstyle_is_accepted_back() {
        let mut app = CadApp::new(Session::new(), Services::default());
        let info = app.session.execute("mlstyle", &json!({})).unwrap();
        let mut s = info["styles"][0].clone();
        s["name"] = json!("WALL");
        s["elements"][0]["offset"] = json!(1.5);
        let mut p = ml_params(&s);
        p["current"] = json!(true);
        app.run("mlstyle", p).unwrap();
        let after = app.session.execute("mlstyle", &json!({})).unwrap();
        assert_eq!(after["current"], json!("WALL"));
        let wall = after["styles"].as_array().unwrap().iter().find(|s| s["name"] == json!("WALL")).unwrap().clone();
        assert_eq!(wall["elements"][0]["offset"], json!(1.5));
    }

    #[test]
    fn a_new_dimension_style_copies_its_base_and_keeps_the_current_one() {
        let mut app = CadApp::new(Session::new(), Services::default());
        let mut vars = dim_vars(&mut app, "Standard");
        assert_eq!(vars.get("DIMCLRD"), Some(&json!(0)), "ByBlock as ACI 0");
        vars.insert("DIMASZ".into(), json!(3.5));
        vars.insert("name".into(), json!("Mech"));
        vars.insert("current".into(), json!(false));
        app.run("dimstyle", Value::Object(vars)).unwrap();
        let r = app.session.execute("dimstyle.list", &json!({ "name": "Mech" })).unwrap();
        assert_eq!(r["current"], json!("Standard"));
        assert_eq!(r["variables"]["DIMASZ"], json!(3.5));
    }

    #[test]
    fn lineweight_labels() {
        assert_eq!(lw_label(-1), "ByLayer");
        assert_eq!(lw_label(25), "0.25 mm");
    }
}
