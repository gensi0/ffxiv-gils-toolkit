// Transport vers le backend. Dans le navigateur (PWA) : HTTP + WebSocket vers
// le serveur. Dans l'app de bureau Tauri (gardée pour un futur client lourd) :
// commandes et événements Tauri, comme avant.
import { invoke as tauriInvoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";

export const IS_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Erreur « non connecté » : l'app affiche l'écran de connexion. */
export class AuthError extends Error {}

const authListeners = new Set<() => void>();
export function onAuthLost(cb: () => void): () => void {
  authListeners.add(cb);
  return () => authListeners.delete(cb);
}

async function http<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(path, {
    method: "POST",
    credentials: "same-origin",
    headers: body === undefined ? {} : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (res.status === 401) {
    for (const cb of authListeners) cb();
    throw new AuthError("Session expirée : reconnecte-toi.");
  }
  if (res.status === 204) return null as T;
  const text = await res.text();
  const data: unknown = text ? JSON.parse(text) : null;
  if (!res.ok) {
    const msg = data && typeof data === "object" && "error" in data ? String(data.error) : text;
    // Même forme que les erreurs Tauri : une chaîne.
    // eslint-disable-next-line @typescript-eslint/only-throw-error
    throw msg || `erreur ${String(res.status)}`;
  }
  return data as T;
}

/** Appelle une commande du backend (mêmes noms et arguments que sous Tauri). */
export function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (IS_TAURI) return tauriInvoke<T>(cmd, args);
  return http<T>(`/api/cmd/${cmd}`, args ?? {});
}

/** Appel brut de l'API du serveur (comptes, notifications). */
export function serverCall<T>(path: string, body?: unknown): Promise<T> {
  return http<T>(path, body);
}

export async function serverGet<T>(path: string): Promise<T> {
  const res = await fetch(path, { credentials: "same-origin" });
  if (res.status === 401) throw new AuthError("non connecté");
  if (!res.ok) throw new Error(await res.text());
  return (await res.json()) as T;
}

// --- Événements -----------------------------------------------------------

type Handler = (e: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
let socket: WebSocket | null = null;
let retry = 1000;
let wanted = false;
const reconnectListeners = new Set<() => void>();

/** Appelé après chaque reconnexion (pour recharger ce qui a pu être manqué). */
export function onReconnect(cb: () => void): () => void {
  reconnectListeners.add(cb);
  return () => reconnectListeners.delete(cb);
}

function connect() {
  if (socket || !wanted) return;
  const url = `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/api/events`;
  const ws = new WebSocket(url);
  socket = ws;
  let opened = false;
  ws.onopen = () => {
    opened = true;
    retry = 1000;
    for (const cb of reconnectListeners) cb();
  };
  ws.onmessage = (msg) => {
    try {
      const { event, payload } = JSON.parse(String(msg.data)) as {
        event: string;
        payload: unknown;
      };
      for (const h of handlers.get(event) ?? []) h({ payload });
    } catch {
      /* message illisible : ignoré */
    }
  };
  ws.onclose = () => {
    socket = null;
    if (!wanted) return;
    if (!opened) retry = Math.min(retry * 2, 30_000);
    setTimeout(connect, retry);
  };
}

/** Ouvre le flux d'événements (après connexion au compte). */
export function startEvents() {
  if (IS_TAURI) return;
  wanted = true;
  connect();
}

export function stopEvents() {
  wanted = false;
  socket?.close();
  socket = null;
}

// Réveil du téléphone ou retour sur l'onglet : reconnexion immédiate.
if (typeof document !== "undefined") {
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible" && wanted && !socket) {
      retry = 1000;
      connect();
    }
  });
}

/** Écoute un événement du backend ; renvoie une fonction de désabonnement. */
// eslint-disable-next-line @typescript-eslint/no-unnecessary-type-parameters -- comme `listen` de Tauri
export function listen<T>(event: string, cb: (e: { payload: T }) => void): Promise<() => void> {
  if (IS_TAURI) return tauriListen<T>(event, cb);
  const set = handlers.get(event) ?? new Set<Handler>();
  handlers.set(event, set);
  const h = cb as Handler;
  set.add(h);
  return Promise.resolve(() => {
    set.delete(h);
  });
}

/** Adresse d'une icône XIVAPI (cache disque côté backend). */
export function iconUrl(path: string): string {
  if (IS_TAURI) return convertFileSrc(path, "icon");
  return `/api/icon/${path}`;
}
