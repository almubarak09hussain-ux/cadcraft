//! Offline use: registers the service worker (`sw.js`, next to index.html) that keeps this build's
//! files on the device, so the app added to the Home Screen opens with no connection. The first
//! visit online fills the cache; later visits online update it to the newest build.

use wasm_bindgen_futures::JsFuture;

/// Register `./sw.js`. Browsers without service workers (or a page not served over HTTPS or from
/// localhost) simply stay online-only; the result is reported on the browser console only.
pub fn register() {
    let Some(window) = web_sys::window() else { return };
    let container = window.navigator().service_worker();
    let promise = container.register("./sw.js");
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = JsFuture::from(promise).await {
            web_sys::console::warn_1(&format!("CADCraft offline copy unavailable: {e:?}").into());
        }
    });
}
