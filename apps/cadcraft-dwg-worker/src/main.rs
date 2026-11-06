//! CADCraft's DWG worker: a Web Worker the web app (`apps/cadcraft-web/src/dwg.rs`) starts for
//! each DWG it opens or saves.
//!
//! The DWG converter runs here instead of in the app because a wasm panic can't be caught: in the
//! app it would end the page and lose the open drawings; here it ends the worker, and the app
//! reports that the file couldn't be read. It also keeps the app responsive while a large DWG
//! converts.
//!
//! Protocol (structured-clone messages): the worker posts `{ready: true}` once it listens; the
//! app then posts one request, `{op: "read", bytes}` (DWG in, DXF out) or
//! `{op: "write", bytes, version}` (DXF in, DWG of `version`, e.g. "AC1032", out), and gets
//! `{ok: true, bytes, notes}` or `{ok: false, error}` back.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

/// What a request asks for, read from the message's fields.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn convert(op: &str, bytes: Option<Vec<u8>>, version: Option<String>) -> Result<(Vec<u8>, Vec<String>), String> {
    let bytes = bytes.ok_or("the request carries no file")?;
    match op {
        "read" => cadcraft_dwg::dwg_to_dxf_noted(&bytes, cadcraft_dwg::Limits::DEFAULT),
        "write" => {
            let version = version.ok_or("the request names no DWG version")?;
            cadcraft_dwg::dxf_to_dwg_as(&bytes, &version).map(|dwg| (dwg, Vec::new()))
        }
        other => Err(format!("unknown DWG worker request `{other}`")),
    }
}

#[cfg(target_arch = "wasm32")]
mod worker {
    use wasm_bindgen::JsCast as _;
    use wasm_bindgen::prelude::*;
    use web_sys::js_sys;

    fn field(data: &JsValue, key: &str) -> Option<JsValue> {
        js_sys::Reflect::get(data, &JsValue::from_str(key)).ok().filter(|v| !v.is_undefined() && !v.is_null())
    }

    fn set(obj: &js_sys::Object, key: &str, value: &JsValue) {
        let _ = js_sys::Reflect::set(obj, &JsValue::from_str(key), value);
    }

    /// Answer one request: the reply and the buffer to transfer with it (not copy).
    fn answer(data: &JsValue) -> (js_sys::Object, Option<js_sys::ArrayBuffer>) {
        let op = field(data, "op").and_then(|v| v.as_string()).unwrap_or_default();
        let bytes = field(data, "bytes").and_then(|v| v.dyn_into::<js_sys::Uint8Array>().ok()).map(|a| a.to_vec());
        let version = field(data, "version").and_then(|v| v.as_string());
        let reply = js_sys::Object::new();
        match super::convert(&op, bytes, version) {
            Ok((out, notes)) => {
                let array = js_sys::Uint8Array::from(out.as_slice());
                set(&reply, "ok", &JsValue::TRUE);
                set(&reply, "bytes", &array);
                set(&reply, "notes", &notes.iter().map(|n| JsValue::from_str(n)).collect::<js_sys::Array>());
                (reply, Some(array.buffer()))
            }
            Err(e) => {
                set(&reply, "ok", &JsValue::FALSE);
                set(&reply, "error", &JsValue::from_str(&e));
                (reply, None)
            }
        }
    }

    pub fn start() {
        let Ok(scope) = js_sys::global().dyn_into::<web_sys::DedicatedWorkerGlobalScope>() else {
            return;
        };
        let replies = scope.clone();
        let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
            let (reply, buffer) = answer(&e.data());
            let sent = match buffer {
                Some(b) => replies.post_message_with_transfer(&reply, &js_sys::Array::of1(&b)),
                None => replies.post_message(&reply),
            };
            if sent.is_err() {
                let failed = js_sys::Object::new();
                set(&failed, "ok", &JsValue::FALSE);
                set(&failed, "error", &JsValue::from_str("the DWG worker could not send its result"));
                let _ = replies.post_message(&failed);
            }
        });
        scope.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        // The worker lives for one request; its handler lives as long as it does.
        on_message.forget();
        let ready = js_sys::Object::new();
        set(&ready, "ready", &JsValue::TRUE);
        let _ = scope.post_message(&ready);
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    worker::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("cadcraft-dwg-worker only runs in the browser, started by the web app (trunk build in apps/cadcraft-web)");
}

#[cfg(test)]
mod tests {
    use super::convert;

    #[test]
    fn bad_requests_are_errors_not_crashes() {
        assert!(convert("read", None, None).is_err());
        assert!(convert("read", Some(b"AC1032 not a drawing".to_vec()), None).is_err());
        assert!(convert("write", Some(Vec::new()), None).is_err());
        assert!(convert("write", Some(Vec::new()), Some("AC9999".into())).is_err());
        assert!(convert("explode", Some(Vec::new()), None).is_err());
    }
}
