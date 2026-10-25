//! Touch on the drawing area (iPad, touch screens): a tap places a point, two fingers pan and
//! pinch without placing one, and a long press is Enter while a command runs. The events are the
//! ones egui-winit sends: a `Touch` per finger, plus the pointer for the first finger.

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services};
use egui::{Event, PointerButton, Pos2, TouchDeviceId, TouchId, TouchPhase, pos2};

fn frame(app: &mut CadApp, ctx: &egui::Context, events: Vec<Event>, time: f64) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1600.0, 1000.0))),
        time: Some(time),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| app.ui(ui));
    output.textures_delta.clear();
}

fn touch(id: u64, phase: TouchPhase, pos: Pos2) -> Event {
    Event::Touch { device_id: TouchDeviceId(0), id: TouchId(id), phase, pos, force: None }
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }
}

/// The first finger down (it is also the pointer).
fn first_down(p: Pos2) -> Vec<Event> {
    vec![touch(1, TouchPhase::Start, p), Event::PointerMoved(p), button(p, true)]
}

/// The first finger up: egui-winit releases the button and forgets the pointer in one frame.
fn first_up(p: Pos2) -> Vec<Event> {
    vec![touch(1, TouchPhase::End, p), button(p, false), Event::PointerGone]
}

fn setup() -> (CadApp, egui::Context) {
    let mut app = CadApp::new(Session::new(), Services::default());
    app.session.settings.dynmode = false;
    let ctx = egui::Context::default();
    app.logic(&ctx);
    frame(&mut app, &ctx, Vec::new(), 0.0);
    frame(&mut app, &ctx, Vec::new(), 0.05);
    (app, ctx)
}

fn view_height(app: &CadApp) -> f64 {
    app.session.state().map(|s| s.view().height).unwrap_or_default()
}

#[test]
fn a_tap_places_a_point_where_the_finger_lifts() {
    let (mut app, ctx) = setup();
    app.cmdline("LINE");
    let at = pos2(800.0, 500.0);
    frame(&mut app, &ctx, first_down(at), 1.0);
    frame(&mut app, &ctx, first_up(at), 1.05);
    frame(&mut app, &ctx, Vec::new(), 1.1);
    assert!(app.session.prompt_text().contains("next point"), "the tap is the first point: {}", app.session.prompt_text());
}

#[test]
fn two_fingers_zoom_without_placing_a_point() {
    let (mut app, ctx) = setup();
    app.cmdline("LINE");
    let before = view_height(&app);
    let (a, b) = (pos2(700.0, 500.0), pos2(900.0, 500.0));
    frame(&mut app, &ctx, first_down(a), 1.0);
    frame(&mut app, &ctx, vec![touch(2, TouchPhase::Start, b)], 1.02);
    // Spread the fingers over a few frames: a pinch out (zoom in).
    for k in 1..=5 {
        let d = 20.0 * k as f32;
        let (a2, b2) = (pos2(a.x - d, a.y), pos2(b.x + d, b.y));
        frame(
            &mut app,
            &ctx,
            vec![touch(1, TouchPhase::Move, a2), Event::PointerMoved(a2), touch(2, TouchPhase::Move, b2)],
            1.02 + 0.02 * f64::from(k),
        );
    }
    let lifted = pos2(a.x - 100.0, a.y);
    frame(&mut app, &ctx, vec![touch(2, TouchPhase::End, pos2(b.x + 100.0, b.y))], 1.2);
    frame(&mut app, &ctx, first_up(lifted), 1.22);
    frame(&mut app, &ctx, Vec::new(), 1.3);
    let after = view_height(&app);
    assert!(after < before * 0.95, "pinching out zooms in: view height {before} -> {after}");
    assert!(app.session.prompt_text().contains("first point"), "no point placed: {}", app.session.prompt_text());
    assert!(app.session.pending_window.is_none());
}

#[test]
fn a_long_press_is_enter_while_a_command_runs() {
    let (mut app, ctx) = setup();
    app.cmdline("LINE 0,0 10,0");
    assert!(app.session.running.is_some());
    let at = pos2(800.0, 500.0);
    frame(&mut app, &ctx, first_down(at), 1.0);
    // Held still: egui reports a long press after its delay.
    for k in 1..=12 {
        frame(&mut app, &ctx, Vec::new(), 1.0 + 0.1 * f64::from(k));
    }
    frame(&mut app, &ctx, first_up(at), 2.3);
    frame(&mut app, &ctx, Vec::new(), 2.4);
    assert!(app.session.running.is_none(), "a long press is Enter: {}", app.session.prompt_text());
    assert_eq!(app.session.doc().map(|d| d.model.handles().len()).unwrap_or_default(), 1, "the lift placed no second segment");
}

#[test]
fn a_finger_gets_larger_pick_boxes_until_the_mouse_moves() {
    let (mut app, ctx) = setup();
    assert_eq!(app.session.pointer_scale(), 1.0);
    let at = pos2(800.0, 500.0);
    frame(&mut app, &ctx, first_down(at), 1.0);
    frame(&mut app, &ctx, first_up(at), 1.05);
    frame(&mut app, &ctx, Vec::new(), 1.1);
    assert_eq!(app.session.pointer_scale(), cadcraft_ui_egui::touch::FINGER_SCALE, "after a tap");
    frame(&mut app, &ctx, vec![Event::PointerMoved(pos2(810.0, 505.0))], 1.2);
    assert_eq!(app.session.pointer_scale(), 1.0, "a mouse move restores PICKBOX's size");
}
