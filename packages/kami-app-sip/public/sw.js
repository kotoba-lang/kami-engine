// Spirit in Physics — service worker (P4-A PWA). App-shell cache so the HUD is
// installable and plays offline (it already runs without a server / WebGPU).
const CACHE = 'sip-v7';
const SHELL = [
  '/', '/index.html', '/js/sip.js?b=7',
  '/snapshot.edn', '/manifest.webmanifest', '/icon.svg',
];

self.addEventListener('install', (e) => {
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(SHELL)).then(() => self.skipWaiting()));
});

self.addEventListener('activate', (e) => {
  e.waitUntil(
    caches.keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener('fetch', (e) => {
  const req = e.request;
  if (req.method !== 'GET' || new URL(req.url).origin !== self.location.origin) return;
  // cache-first for the app shell; fall back to network, then to cached index for navigations.
  e.respondWith(
    caches.match(req).then((hit) =>
      hit || fetch(req)
        .then((res) => {
          const copy = res.clone();
          caches.open(CACHE).then((c) => c.put(req, copy)).catch(() => {});
          return res;
        })
        .catch(() => (req.mode === 'navigate' ? caches.match('/index.html') : undefined))
    )
  );
});
