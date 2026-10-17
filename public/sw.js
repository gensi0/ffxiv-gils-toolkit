// Service worker du client web : installation (PWA), cache de l'interface
// pour un démarrage rapide, et notifications push du serveur.
const CACHE = "gils-toolkit-v1";

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((c) => c.addAll(["/", "/manifest.webmanifest", "/icon-192.png"]))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const req = event.request;
  const url = new URL(req.url);
  if (req.method !== "GET" || url.origin !== location.origin || url.pathname.startsWith("/api/"))
    return;
  // Fichiers versionnés (/assets/…) : cache d'abord. Le reste (index.html) :
  // réseau d'abord, cache si hors ligne, pour toujours charger la dernière version.
  if (url.pathname.startsWith("/assets/")) {
    event.respondWith(
      caches.match(req).then(
        (hit) =>
          hit ??
          fetch(req).then((res) => {
            const copy = res.clone();
            void caches.open(CACHE).then((c) => c.put(req, copy));
            return res;
          }),
      ),
    );
    return;
  }
  event.respondWith(
    fetch(req)
      .then((res) => {
        if (res.ok && req.mode === "navigate") {
          const copy = res.clone();
          void caches.open(CACHE).then((c) => c.put("/", copy));
        }
        return res;
      })
      .catch(() =>
        caches.match(req.mode === "navigate" ? "/" : req).then((r) => r ?? Response.error()),
      ),
  );
});

self.addEventListener("push", (event) => {
  let data = { title: "FFXIV Gils Toolkit", body: "" };
  try {
    data = { ...data, ...event.data.json() };
  } catch {
    /* charge utile vide ou illisible */
  }
  event.waitUntil(
    self.registration.showNotification(data.title, {
      body: data.body,
      icon: "/icon-192.png",
      badge: "/badge-96.png",
      tag: data.title + data.body,
    }),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  event.waitUntil(
    self.clients.matchAll({ type: "window", includeUncontrolled: true }).then((list) => {
      const open = list.find((c) => "focus" in c);
      return open ? open.focus() : self.clients.openWindow("/");
    }),
  );
});
