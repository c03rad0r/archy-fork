const CACHE_VERSION = "gashboard-pwa-v4";
const APP_SHELL = ["/", "/manifest.webmanifest", "/icons/icon.svg", "/icons/maskable.svg"];
const MINER_ASSETS = [
  "/miners/nerdqaxe.svg",
  "/miners/bitaxe.svg",
  "/miners/bigpapa-bitaxe.svg",
  "/miners/avalon-nano-3.svg",
  "/miners/avalon-mini-3.svg",
  "/miners/boomer-heater.svg",
];
const STICKER_ASSETS = [
  "/stickers/anon-miner.svg",
  "/stickers/block-denied.svg",
  "/stickers/block-hopium.svg",
  "/stickers/boomer-unit.svg",
  "/stickers/chad-hash.svg",
  "/stickers/chart-crime.svg",
  "/stickers/fan-panic.svg",
  "/stickers/fiat-npc.svg",
  "/stickers/heat-death.svg",
  "/stickers/money-printer.svg",
  "/stickers/odds-wizard.svg",
  "/stickers/printer-loop.svg",
];

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE_VERSION).then((cache) => cache.addAll([...APP_SHELL, ...MINER_ASSETS, ...STICKER_ASSETS])));
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((key) => key !== CACHE_VERSION).map((key) => caches.delete(key))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const req = event.request;
  if (req.method !== "GET") return;

  const url = new URL(req.url);
  if (url.origin !== self.location.origin) return;
  if (url.pathname === "/nostr-provider.js" || url.pathname === "/nostr-signer") return;
  if (url.pathname.startsWith("/api/") || url.pathname === "/healthz") return;

  if (req.mode === "navigate") {
    event.respondWith(networkFirst(req, "/"));
    return;
  }

  if (
    url.pathname.startsWith("/assets/")
    || url.pathname.startsWith("/icons/")
    || url.pathname.startsWith("/miners/")
    || url.pathname.startsWith("/stickers/")
  ) {
    event.respondWith(cacheFirst(req));
    return;
  }

  event.respondWith(networkFirst(req));
});

self.addEventListener("message", (event) => {
  if (event.data?.type === "SKIP_WAITING") self.skipWaiting();
});

async function cacheFirst(req) {
  const cache = await caches.open(CACHE_VERSION);
  const cached = await cache.match(req);
  if (cached) return cached;
  const res = await fetch(req);
  if (res.ok) cache.put(req, res.clone());
  return res;
}

async function networkFirst(req, fallbackPath) {
  const cache = await caches.open(CACHE_VERSION);
  try {
    const res = await fetch(req);
    if (res.ok) cache.put(req, res.clone());
    return res;
  } catch {
    const cached = await cache.match(req);
    if (cached) return cached;
    if (fallbackPath) {
      const fallback = await cache.match(fallbackPath);
      if (fallback) return fallback;
    }
    throw new Error("offline");
  }
}
