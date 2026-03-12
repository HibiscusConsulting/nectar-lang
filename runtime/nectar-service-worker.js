/**
 * Nectar Service Worker — built-in offline-first service worker for Nectar apps.
 *
 * Features:
 * - Cache-first for app shell (HTML, CSS, JS, WASM)
 * - Network-first for API calls with cache fallback
 * - Aggressive WASM caching with long-lived storage
 * - Offline fallback page
 * - Auto-versioning stamped by the Nectar compiler
 *
 * This file is self-contained — no external dependencies.
 */

// Stamped by the Nectar compiler at build time
const CACHE_VERSION = "__ARC_CACHE_VERSION__";
const APP_CACHE = `nectar-app-v${CACHE_VERSION}`;
const WASM_CACHE = `nectar-wasm-v${CACHE_VERSION}`;
const API_CACHE = `nectar-api-v${CACHE_VERSION}`;

// Populated by the Nectar compiler from the build manifest
const PRECACHE_URLS = self.__ARC_PRECACHE_MANIFEST__ || [];

const OFFLINE_PAGE = "/offline.html";

const APP_SHELL_EXTENSIONS = [".html", ".css", ".js", ".svg", ".ico", ".png", ".jpg", ".webp"];

// --- Install: precache app shell and WASM ---

self.addEventListener("install", (event) => {
  event.waitUntil(
    Promise.all([
      caches.open(APP_CACHE).then((cache) => {
        const appUrls = PRECACHE_URLS.filter((url) => !url.endsWith(".wasm"));
        return cache.addAll(appUrls);
      }),
      caches.open(WASM_CACHE).then((cache) => {
        const wasmUrls = PRECACHE_URLS.filter((url) => url.endsWith(".wasm"));
        return cache.addAll(wasmUrls);
      }),
    ]).then(() => self.skipWaiting())
  );
});

// --- Activate: clean old caches ---

self.addEventListener("activate", (event) => {
  const currentCaches = new Set([APP_CACHE, WASM_CACHE, API_CACHE]);
  event.waitUntil(
    caches.keys().then((names) =>
      Promise.all(
        names
          .filter((name) => name.startsWith("nectar-") && !currentCaches.has(name))
          .map((name) => caches.delete(name))
      )
    ).then(() => self.clients.claim())
  );
});

// --- Fetch strategies ---

self.addEventListener("fetch", (event) => {
  const { request } = event;
  const url = new URL(request.url);

  // Skip non-GET requests
  if (request.method !== "GET") return;

  // Skip cross-origin requests
  if (url.origin !== self.location.origin) return;

  // WASM files: cache-first with aggressive caching
  if (url.pathname.endsWith(".wasm")) {
    event.respondWith(cacheFirst(request, WASM_CACHE));
    return;
  }

  // API calls: network-first with cache fallback
  if (url.pathname.startsWith("/api/")) {
    event.respondWith(networkFirst(request, API_CACHE));
    return;
  }

  // App shell assets: cache-first
  if (isAppShellAsset(url.pathname)) {
    event.respondWith(cacheFirst(request, APP_CACHE));
    return;
  }

  // Navigation requests: network-first with offline fallback
  if (request.mode === "navigate") {
    event.respondWith(navigationFallback(request));
    return;
  }

  // Everything else: network-first
  event.respondWith(networkFirst(request, APP_CACHE));
});

// --- Strategy implementations ---

async function cacheFirst(request, cacheName) {
  const cached = await caches.match(request);
  if (cached) return cached;
  try {
    const response = await fetch(request);
    if (response.ok) {
      const cache = await caches.open(cacheName);
      cache.put(request, response.clone());
    }
    return response;
  } catch (err) {
    return new Response("Network error", { status: 503, statusText: "Service Unavailable" });
  }
}

async function networkFirst(request, cacheName) {
  try {
    const response = await fetch(request);
    if (response.ok) {
      const cache = await caches.open(cacheName);
      cache.put(request, response.clone());
    }
    return response;
  } catch (err) {
    const cached = await caches.match(request);
    if (cached) return cached;
    return new Response(JSON.stringify({ error: "offline" }), {
      status: 503,
      headers: { "Content-Type": "application/json" },
    });
  }
}

async function navigationFallback(request) {
  try {
    const response = await fetch(request);
    if (response.ok) {
      const cache = await caches.open(APP_CACHE);
      cache.put(request, response.clone());
    }
    return response;
  } catch (err) {
    const cached = await caches.match(request);
    if (cached) return cached;
    const offline = await caches.match(OFFLINE_PAGE);
    if (offline) return offline;
    return new Response("<h1>Offline</h1><p>Please check your connection.</p>", {
      status: 503,
      headers: { "Content-Type": "text/html" },
    });
  }
}

function isAppShellAsset(pathname) {
  return APP_SHELL_EXTENSIONS.some((ext) => pathname.endsWith(ext));
}

// --- Message handling for runtime communication ---

self.addEventListener("message", (event) => {
  if (event.data === "nectar:skipWaiting") {
    self.skipWaiting();
  }
  if (event.data === "nectar:getCacheVersion") {
    event.source.postMessage({ type: "nectar:cacheVersion", version: CACHE_VERSION });
  }
});
