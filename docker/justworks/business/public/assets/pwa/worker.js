// APP is supplied by the dedicated root worker. Never intercept non-GET writes,
// APIs, identities, QR endpoints, merchant photos or third-party requests.
const VERSION = 'v1';
const PREFIX = `jw-${self.APP}-shell-`;
const CACHE = `${PREFIX}${VERSION}`;
const OFFLINE = '/assets/pwa/offline.html';
const SHELL = [OFFLINE, `/assets/pwa/${self.APP}-192.png`, `/assets/pwa/${self.APP}-512.png`];
self.addEventListener('install', event => {
  event.waitUntil(caches.open(CACHE).then(cache => cache.addAll(SHELL)));
  // No skipWaiting: an update never reloads an in-progress POS interaction.
});
self.addEventListener('activate', event => {
  event.waitUntil(caches.keys().then(keys => Promise.all(keys.filter(key => key.startsWith(PREFIX) && key !== CACHE).map(key => caches.delete(key)))).then(() => self.clients.claim()));
});
self.addEventListener('fetch', event => {
  const request = event.request, url = new URL(request.url);
  if (request.method !== 'GET' || url.origin !== self.location.origin) return;
  if (request.mode === 'navigate') {
    // Network-only HTML; the only fallback contains no merchant or live data.
    event.respondWith(fetch(request, {cache: 'no-store'}).catch(async () => (await caches.match(OFFLINE, {cacheName: CACHE})) || Response.error()));
  } else if (!url.search && SHELL.includes(url.pathname)) {
    event.respondWith(caches.open(CACHE).then(async cache => (await cache.match(request)) || fetch(request)));
  }
});
