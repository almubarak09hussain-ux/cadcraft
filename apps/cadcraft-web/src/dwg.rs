//! DWG on the web, through the DWG worker (`apps/cadcraft-dwg-worker`).
//!
//! The DWG converter is not linked into the app: a wasm panic in it couldn't be caught and would
//! end the page with every open drawing. Each DWG opened or saved starts a Web Worker that
//! converts it (DWG → DXF to open, DXF → DWG to save); if the converter fails on the file, only
//! that worker stops and the app reports it. The app stays responsive while a large DWG converts.
//!
//! Saving: the engine's write hook can't wait for a worker, so for a `.dwg` name the web app's
//! write hook ([`write`]) returns the DXF the converter reads and remembers the version to write;
//! the delivery hook ([`deliver`]) sends that DXF to a worker and downloads the DWG it returns.

use std::cell::RefCell;
use std::rc::Rc;

use cadcraft_engine::doc::Drawing;
use wasm_bindgen::JsCast as _;
use wasm_bindgen::prelude::*;
use web_sys::js_sys;

/// The worker's loader, written by trunk next to the app (`index.html`'s worker link; worker
/// file names are not hashed).
const WORKER_URL: &str = "./cadcraft-dwg-worker_loader.js";

/// A conversion's result: the converted bytes and notes on what the read left out.
pub type Converted = Result<(Vec<u8>, Vec<String>), String>;

/// Why a worker that stopped without answering couldn't convert the file.
const STOPPED: &str = "the DWG reader stopped on this file (it may be damaged, or use something CADCraft can't read yet)";

thread_local! {
    /// The DWG version code the last `.dwg` write asked for, for the delivery that follows it.
    static PENDING_VERSION: RefCell<Option<&'static str>> = const { RefCell::new(None) };
    /// Messages for the command line from saves that finished after their SAVE returned.
    static NOTICES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// Repaints the app when a notice arrives.
    static REPAINT: RefCell<Option<egui::Context>> = const { RefCell::new(None) };
}

/// True for the file names saved as DWG (a template, `.dwt`, is DWG content).
pub fn is_dwg_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".dwg") || lower.ends_with(".dwt")
}

/// Let notices from finished saves repaint the app.
pub fn set_repaint(ctx: &egui::Context) {
    REPAINT.with(|r| *r.borrow_mut() = Some(ctx.clone()));
}

/// Notices from saves that finished since the last frame, for the command line.
pub fn take_notices() -> Vec<String> {
    NOTICES.with(|n| std::mem::take(&mut *n.borrow_mut()))
}

fn notice(msg: String) {
    NOTICES.with(|n| n.borrow_mut().push(msg));
    REPAINT.with(|r| {
        if let Some(ctx) = &*r.borrow() {
            ctx.request_repaint();
        }
    });
}

/// The web's write hook: `.dwg` names give the DXF the DWG converter reads (converted in a worker
/// when [`deliver`] gets it), every other format is written at once.
pub fn write(d: &Drawing, name: &str) -> Result<Vec<u8>, String> {
    write_with(d, name, &cadcraft_io::SaveOptions::default())
}

/// [`write`] with SAVEAS options (version, binary DXF).
pub fn write_with(d: &Drawing, name: &str, opts: &cadcraft_io::SaveOptions) -> Result<Vec<u8>, String> {
    if !is_dwg_name(name) {
        return cadcraft_io::write_with(d, name, opts).map_err(|e| e.to_string());
    }
    if opts.binary {
        return Err("binary output is for DXF files, not DWG".into());
    }
    let (dxf, version) = cadcraft_io::dwg_bridge(d, opts);
    PENDING_VERSION.with(|v| *v.borrow_mut() = Some(version));
    Ok(dxf)
}

/// The web's delivery hook: a DWG save converts in a worker, then downloads; everything else
/// downloads at once. A DWG conversion that fails reports on the command line.
pub fn deliver(name: &str, mime: &str, bytes: &[u8]) -> Result<(), String> {
    let version = PENDING_VERSION.with(|v| v.borrow_mut().take());
    let Some(version) = version.filter(|_| is_dwg_name(name) && !cadcraft_dwg::is_dwg(bytes)) else {
        return crate::download::download(name, mime, bytes);
    };
    let file = name.to_string();
    convert("write", Some(version), bytes.to_vec(), move |r| match r {
        Ok((dwg, _)) => {
            if let Err(e) = crate::download::download(&file, "application/acad", &dwg) {
                notice(format!("Save: {file} was not saved: {e}"));
            }
        }
        Err(e) => notice(format!("Save: {file} was not saved: {e}. Save it as DXF instead.")),
    })
}

/// Read a DWG file in a worker; `done` gets the DXF it converts to and the read's notes.
pub fn read(bytes: Vec<u8>, done: impl FnOnce(Converted) + 'static) -> Result<(), String> {
    convert("read", None, bytes, done)
}

/// One worker, one request, then the worker ends.
struct Job {
    worker: web_sys::Worker,
    request: Option<js_sys::Object>,
    done: Option<Box<dyn FnOnce(Converted)>>,
    handlers: Vec<Closure<dyn FnMut(JsValue)>>,
}

fn convert(op: &str, version: Option<&str>, bytes: Vec<u8>, done: impl FnOnce(Converted) + 'static) -> Result<(), String> {
    let worker = web_sys::Worker::new(WORKER_URL).map_err(|e| format!("the DWG worker could not start ({})", js_error(e)))?;
    let request = js_sys::Object::new();
    set(&request, "op", &JsValue::from_str(op));
    set(&request, "bytes", &js_sys::Uint8Array::from(bytes.as_slice()));
    if let Some(v) = version {
        set(&request, "version", &JsValue::from_str(v));
    }
    let job = Rc::new(RefCell::new(Job { worker: worker.clone(), request: Some(request), done: Some(Box::new(done)), handlers: Vec::new() }));

    let on_job = Rc::clone(&job);
    let on_message = Closure::<dyn FnMut(JsValue)>::new(move |e: JsValue| {
        let data = e.dyn_into::<web_sys::MessageEvent>().map(|m| m.data()).unwrap_or(JsValue::UNDEFINED);
        if field(&data, "ready").is_some() {
            send(&on_job);
        } else {
            finish(&on_job, reply(&data));
        }
    });
    let on_fail = Rc::clone(&job);
    let on_error = Closure::<dyn FnMut(JsValue)>::new(move |e: JsValue| {
        // An uncaught error in the worker: the converter failed on this file (a wasm panic
        // ends as a trap) or the worker's files could not load.
        if let Some(e) = e.dyn_ref::<web_sys::Event>() {
            e.prevent_default();
        }
        finish(&on_fail, Err(STOPPED.to_string()));
    });
    worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    if let Ok(mut j) = job.try_borrow_mut() {
        j.handlers = vec![on_message, on_error];
    }
    Ok(())
}

/// The worker listens: send it the request (moving the file's buffer instead of copying it).
fn send(job: &Rc<RefCell<Job>>) {
    let Some((worker, request)) = job.try_borrow_mut().ok().and_then(|mut j| Some((j.worker.clone(), j.request.take()?))) else { return };
    let buffer = field(&request, "bytes").and_then(|b| b.dyn_into::<js_sys::Uint8Array>().ok()).map(|a| a.buffer());
    let sent = match buffer {
        Some(b) => worker.post_message_with_transfer(&request, &js_sys::Array::of1(&b)),
        None => worker.post_message(&request),
    };
    if let Err(e) = sent {
        finish(job, Err(format!("the DWG worker could not get the file ({})", js_error(e))));
    }
}

/// End the worker and report its result, once.
fn finish(job: &Rc<RefCell<Job>>, r: Converted) {
    let Ok(mut j) = job.try_borrow_mut() else { return };
    j.worker.set_onmessage(None);
    j.worker.set_onerror(None);
    j.worker.terminate();
    let done = j.done.take();
    // Dropping a handler while it runs is safe: wasm-bindgen frees it when the call returns.
    let handlers = std::mem::take(&mut j.handlers);
    drop(j);
    drop(handlers);
    if let Some(done) = done {
        done(r);
    }
}

fn reply(data: &JsValue) -> Converted {
    if field(data, "ok").and_then(|v| v.as_bool()) != Some(true) {
        return Err(field(data, "error").and_then(|v| v.as_string()).unwrap_or_else(|| STOPPED.to_string()));
    }
    let bytes = field(data, "bytes").and_then(|b| b.dyn_into::<js_sys::Uint8Array>().ok()).map(|a| a.to_vec()).ok_or_else(|| STOPPED.to_string())?;
    let notes = field(data, "notes")
        .and_then(|n| n.dyn_into::<js_sys::Array>().ok())
        .map(|a| a.iter().filter_map(|v| v.as_string()).collect())
        .unwrap_or_default();
    Ok((bytes, notes))
}

fn field(data: &JsValue, key: &str) -> Option<JsValue> {
    js_sys::Reflect::get(data, &JsValue::from_str(key)).ok().filter(|v| !v.is_undefined() && !v.is_null())
}

fn set(obj: &js_sys::Object, key: &str, value: &JsValue) {
    let _ = js_sys::Reflect::set(obj, &JsValue::from_str(key), value);
}

fn js_error(e: JsValue) -> String {
    e.as_string().unwrap_or_else(|| format!("{e:?}"))
}
