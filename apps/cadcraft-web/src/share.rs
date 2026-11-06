//! Files for the user on a tablet: the share sheet's "Save to Files", where the user picks the
//! folder (an iPad's Safari downloads only to its Downloads folder otherwise).
//!
//! Browsers open the share sheet only from a tap on the page itself, so files CADCraft produces
//! (saves, plots, every sheet of a Batch Publish) wait in a small bar over the app: "Save 3 files
//! to Files…". Tapping it shares them all at once; ✕ drops them. Where the browser can't share
//! files (desktop browsers), they download as before.

use std::cell::RefCell;

use wasm_bindgen::JsCast as _;
use wasm_bindgen::prelude::*;
use web_sys::js_sys;

const BAR_ID: &str = "cadcraft_share_bar";
/// Most files waiting at once (older ones are downloaded instead).
const MAX_WAITING: usize = 64;

thread_local! {
    /// Files waiting for the bar to be tapped: (name, MIME type, bytes).
    static WAITING: RefCell<Vec<(String, String, Vec<u8>)>> = const { RefCell::new(Vec::new()) };
}

/// Deliver a file: wait in the share bar where files can be shared (tablets), else download it.
pub fn deliver(name: &str, mime: &str, bytes: &[u8]) -> Result<(), String> {
    if !can_share_files() {
        return crate::download::download(name, mime, bytes);
    }
    let full = WAITING.with(|w| w.borrow().len() >= MAX_WAITING);
    if full {
        return crate::download::download(name, mime, bytes);
    }
    WAITING.with(|w| w.borrow_mut().push((name.to_string(), mime.to_string(), bytes.to_vec())));
    show_bar()
}

fn navigator() -> Option<web_sys::Navigator> {
    web_sys::window().map(|w| w.navigator())
}

/// True where the browser shares files (Safari on iPad and iPhone, Chrome on Android) and the
/// device is a touch device (desktop browsers that share files still download, as users expect).
fn can_share_files() -> bool {
    let Some(nav) = navigator() else { return false };
    if nav.max_touch_points() == 0 {
        return false;
    }
    let Ok(can) = js_sys::Reflect::get(&nav, &JsValue::from_str("canShare")) else { return false };
    let Some(can) = can.dyn_ref::<js_sys::Function>() else { return false };
    // A test file of the type CADCraft shares.
    let Some(data) = share_data(&[("test.pdf".into(), "application/pdf".into(), Vec::new())]) else { return false };
    can.call1(&nav, &data).ok().and_then(|v| v.as_bool()).unwrap_or(false)
}

/// `{files: [File, …]}` for `navigator.share`.
fn share_data(files: &[(String, String, Vec<u8>)]) -> Option<js_sys::Object> {
    let list = js_sys::Array::new();
    for (name, mime, bytes) in files {
        let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes.as_slice()));
        let opts = web_sys::FilePropertyBag::new();
        opts.set_type(mime);
        let file = web_sys::File::new_with_u8_array_sequence_and_options(&parts, name, &opts).ok()?;
        list.push(&file);
    }
    let data = js_sys::Object::new();
    js_sys::Reflect::set(&data, &JsValue::from_str("files"), &list).ok()?;
    Some(data)
}

/// Show (or update) the bar with the waiting files' count.
fn show_bar() -> Result<(), String> {
    let document = web_sys::window().and_then(|w| w.document()).ok_or("no browser document")?;
    let body = document.body().ok_or("the page has no body")?;
    let count = WAITING.with(|w| w.borrow().len());
    let label = if count == 1 {
        let name = WAITING.with(|w| w.borrow().first().map(|f| f.0.clone()).unwrap_or_default());
        format!("Save {name} to Files…")
    } else {
        format!("Save {count} files to Files…")
    };
    if let Some(bar) = document.get_element_by_id(BAR_ID) {
        if let Some(button) = bar.first_element_child() {
            button.set_text_content(Some(&label));
        }
        return Ok(());
    }
    let bar = document.create_element("div").map_err(js_error)?;
    bar.set_id(BAR_ID);
    bar.set_attribute(
        "style",
        "position:fixed;left:50%;bottom:64px;transform:translateX(-50%);z-index:1000;display:flex;gap:6px;\
         background:#2b3440;border:1px solid #4a90d9;border-radius:8px;padding:6px;box-shadow:0 4px 16px rgba(0,0,0,.5);\
         font:15px system-ui,sans-serif",
    )
    .map_err(js_error)?;
    let save = document.create_element("button").map_err(js_error)?;
    save.set_text_content(Some(&label));
    save.set_attribute("style", "background:#4a90d9;color:#fff;border:0;border-radius:6px;padding:10px 16px;font:inherit").map_err(js_error)?;
    let close = document.create_element("button").map_err(js_error)?;
    close.set_text_content(Some("✕"));
    close.set_attribute("aria-label", "Discard").map_err(js_error)?;
    close.set_attribute("style", "background:transparent;color:#ccc;border:0;padding:10px 12px;font:inherit").map_err(js_error)?;
    let on_save = Closure::<dyn FnMut()>::new(share_waiting);
    save.add_event_listener_with_callback("click", on_save.as_ref().unchecked_ref()).map_err(js_error)?;
    on_save.forget();
    let on_close = Closure::<dyn FnMut()>::new(|| {
        WAITING.with(|w| w.borrow_mut().clear());
        remove_bar();
    });
    close.add_event_listener_with_callback("click", on_close.as_ref().unchecked_ref()).map_err(js_error)?;
    on_close.forget();
    bar.append_child(&save).map_err(js_error)?;
    bar.append_child(&close).map_err(js_error)?;
    body.append_child(&bar).map_err(js_error)?;
    Ok(())
}

fn remove_bar() {
    if let Some(bar) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id(BAR_ID)) {
        bar.remove();
    }
}

/// The bar was tapped: open the share sheet with every waiting file. Shared or not, the files
/// stay until the sheet closes; a cancelled sheet leaves them to tap again.
fn share_waiting() {
    let files = WAITING.with(|w| w.borrow().clone());
    let (Some(nav), Some(data)) = (navigator(), share_data(&files)) else { return };
    let Ok(share) = js_sys::Reflect::get(&nav, &JsValue::from_str("share")) else { return };
    let Some(share) = share.dyn_ref::<js_sys::Function>() else { return };
    let Ok(promise) = share.call1(&nav, &data) else { return };
    let Ok(promise) = promise.dyn_into::<js_sys::Promise>() else { return };
    wasm_bindgen_futures::spawn_local(async move {
        let shared = wasm_bindgen_futures::JsFuture::from(promise).await.is_ok();
        if shared {
            WAITING.with(|w| w.borrow_mut().clear());
            remove_bar();
        }
    });
}

fn js_error(e: JsValue) -> String {
    e.as_string().unwrap_or_else(|| format!("{e:?}"))
}
