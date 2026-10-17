// Notifications push (client web) : abonnement de cet appareil auprès du
// serveur, qui envoie les alertes même quand l'app est fermée.
import { IS_TAURI, serverCall, serverGet } from "@/lib/backend";

export const PUSH_SUPPORTED =
  typeof window !== "undefined" &&
  !IS_TAURI &&
  "serviceWorker" in navigator &&
  "PushManager" in window &&
  "Notification" in window;

function keyBytes(b64: string): Uint8Array<ArrayBuffer> {
  const pad = "=".repeat((4 - (b64.length % 4)) % 4);
  const raw = atob((b64 + pad).replace(/-/g, "+").replace(/_/g, "/"));
  const out = new Uint8Array(new ArrayBuffer(raw.length));
  for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
  return out;
}

async function registration(): Promise<ServiceWorkerRegistration> {
  return navigator.serviceWorker.ready;
}

/** Cet appareil reçoit-il les notifications ? */
export async function pushEnabled(): Promise<boolean> {
  if (!PUSH_SUPPORTED || Notification.permission !== "granted") return false;
  const sub = await (await registration()).pushManager.getSubscription();
  return sub !== null;
}

export async function enablePush(): Promise<void> {
  if (!PUSH_SUPPORTED) throw new Error("Ce navigateur ne gère pas les notifications push.");
  const permission = await Notification.requestPermission();
  if (permission !== "granted") throw new Error("Notifications refusées dans le navigateur.");
  const { key } = await serverGet<{ key: string }>("/api/push/key");
  const reg = await registration();
  const existing = await reg.pushManager.getSubscription();
  const sub =
    existing ??
    (await reg.pushManager.subscribe({
      userVisibleOnly: true,
      applicationServerKey: keyBytes(key),
    }));
  await serverCall("/api/push/subscribe", sub.toJSON());
}

export async function disablePush(): Promise<void> {
  const sub = await (await registration()).pushManager.getSubscription();
  if (!sub) return;
  await serverCall("/api/push/unsubscribe", { endpoint: sub.endpoint }).catch(() => undefined);
  await sub.unsubscribe();
}

export function testPush(): Promise<null> {
  return serverCall<null>("/api/push/test");
}

/** Enregistre le service worker (PWA : installation, cache, push). */
export function registerServiceWorker() {
  if (IS_TAURI || !("serviceWorker" in navigator) || import.meta.env.DEV) return;
  window.addEventListener("load", () => {
    void navigator.serviceWorker.register("/sw.js");
  });
}
