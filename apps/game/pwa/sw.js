// Un-Banished's service worker: lets the browser install the game as an app (issue #13) and
// start it without a network. It fetches Un-Banished's own files from the network first and keeps
// a copy for offline use. It never keeps the game data (.pkg) or mods: the player chooses those,
// and the game keeps them in the browser's storage itself.
const CACHE = 'un-banished-v1';

self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener('fetch', (event) => {
  const request = event.request;
  const url = new URL(request.url);
  if (request.method !== 'GET' || url.origin !== self.location.origin) return;
  // Game data served for development (?data=…) and any package or mod file: never kept.
  if (url.pathname.includes('/game-data/') || /\.(pkg|pkm|sav)$/i.test(url.pathname)) return;
  event.respondWith(
    fetch(request)
      .then((response) => {
        if (response.ok) {
          const copy = response.clone();
          caches.open(CACHE).then((cache) => cache.put(request, copy));
        }
        return response;
      })
      .catch(() => caches.match(request, { ignoreSearch: true })),
  );
});
