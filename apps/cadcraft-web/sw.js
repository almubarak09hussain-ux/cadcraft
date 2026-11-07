// CADCraft offline: keeps this build's files on the device so the app opens with no connection.
//
// The one handwritten script in CADCraft, an exception to the Rust-only rule: a service worker
// has to be a script file of its own. It is registered from Rust (src/offline.rs), and does
// nothing but cache: the page, the app's code and the DWG worker's.
//
// The page (index.html) and the DWG worker's files keep their names from build to build, so they
// come from the network when there is one (a newer build), else from the cache. The app's own
// files have the build's hash in their names, so a cached copy is always the right one.
const CACHE = "cadcraft-offline-v1";
const WORKER_FILES = ["./cadcraft-dwg-worker_loader.js", "./cadcraft-dwg-worker.js", "./cadcraft-dwg-worker_bg.wasm"];
const PAGE = "./";
// How long a page request waits for the network before the cached copy is used.
const PAGE_TIMEOUT_MS = 4000;

function scoped(path) {
  return new URL(path, self.registration.scope).href;
}

// The files a page loads: its script and wasm references ("./name.js", "./name_bg.wasm").
function assetsOf(html) {
  const found = new Set();
  for (const m of html.matchAll(/["'](\.\/[A-Za-z0-9_.-]+\.(?:js|wasm))["']/g)) found.add(m[1]);
  return [...found];
}

// Cache `page` (a fresh index.html response) and every file it and the DWG worker need, then
// drop files of older builds.
async function store(page) {
  const cache = await caches.open(CACHE);
  const html = await page.clone().text();
  const keep = new Set([scoped(PAGE), ...assetsOf(html).map(scoped), ...WORKER_FILES.map(scoped)]);
  await cache.put(scoped(PAGE), page);
  for (const url of keep) {
    if (url === scoped(PAGE)) continue;
    const named = !WORKER_FILES.map(scoped).includes(url);
    if (named && (await cache.match(url))) continue;
    try {
      const r = await fetch(url, { cache: "no-cache" });
      if (r.ok) await cache.put(url, r);
    } catch (_) {
      // Offline part-way: what is cached stays; the next visit online fills the rest.
    }
  }
  for (const req of await cache.keys()) {
    if (!keep.has(req.url)) await cache.delete(req);
  }
}

self.addEventListener("install", (event) => {
  event.waitUntil(
    fetch(scoped(PAGE), { cache: "no-cache" })
      .then((r) => (r.ok ? store(r) : undefined))
      .catch(() => undefined)
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(self.clients.claim());
});

function withTimeout(promise, ms) {
  return new Promise((resolve, reject) => {
    const t = setTimeout(() => reject(new Error("timeout")), ms);
    promise.then((v) => { clearTimeout(t); resolve(v); }, (e) => { clearTimeout(t); reject(e); });
  });
}

self.addEventListener("fetch", (event) => {
  const req = event.request;
  if (req.method !== "GET" || new URL(req.url).origin !== self.location.origin) return;
  const url = new URL(req.url);
  const isPage = req.mode === "navigate" || url.href === scoped(PAGE) || url.href === scoped("./index.html");
  const isWorker = WORKER_FILES.map(scoped).includes(url.origin + url.pathname);
  if (isPage) {
    // Network first (a newer build), the cached page offline or on a slow connection.
    event.respondWith(
      withTimeout(fetch(req, { cache: "no-cache" }), PAGE_TIMEOUT_MS)
        .then((r) => {
          if (r.ok) event.waitUntil(store(r.clone()));
          return r;
        })
        .catch(async () => (await caches.match(scoped(PAGE))) || Response.error()),
    );
  } else if (isWorker) {
    event.respondWith(
      fetch(req)
        .then(async (r) => {
          if (r.ok) await (await caches.open(CACHE)).put(url.origin + url.pathname, r.clone());
          return r;
        })
        .catch(async () => (await caches.match(url.origin + url.pathname)) || Response.error()),
    );
  } else {
    // Build-hashed files: the cached copy, else the network (kept for next time).
    event.respondWith(
      caches.match(req).then(
        (hit) =>
          hit ||
          fetch(req).then(async (r) => {
            if (r.ok && /\.(js|wasm)$/.test(url.pathname)) await (await caches.open(CACHE)).put(req, r.clone());
            return r;
          }),
      ),
    );
  }
});
