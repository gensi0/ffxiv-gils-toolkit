// Stockage des stores persistants (liste de courses, préférences…) : sur le
// serveur, rattaché au compte, pour les retrouver sur tous les appareils.
// Dans l'app de bureau, localStorage comme avant.
import type { StateStorage } from "zustand/middleware";
import { IS_TAURI, invoke, listen } from "@/lib/backend";

const pending = new Map<string, ReturnType<typeof setTimeout>>();

export const accountStorage: StateStorage = IS_TAURI
  ? localStorage
  : {
      getItem: (name) => invoke<string | null>("kv_get", { key: name }),
      setItem: (name, value) => {
        // Regroupe les écritures rapprochées (frappe, cases cochées en série).
        clearTimeout(pending.get(name));
        pending.set(
          name,
          setTimeout(() => {
            pending.delete(name);
            void invoke("kv_set", { key: name, value }).catch(() => undefined);
          }, 400),
        );
      },
      removeItem: (name) => {
        void invoke("kv_set", { key: name, value: "null" }).catch(() => undefined);
      },
    };

/** Web : chargement différé jusqu'à la connexion (voir `hydrateAccountStores`). */
export const SKIP_HYDRATION = !IS_TAURI;

const stores: Rehydratable[] = [];

/** Charge les stores du compte, une fois connecté. */
export async function hydrateAccountStores() {
  for (const s of stores) await Promise.resolve(s.persist.rehydrate());
}

interface Rehydratable {
  persist: { rehydrate: () => Promise<void> | void; getOptions: () => { name?: string } };
}

/**
 * Recharge un store quand un autre appareil le modifie (événement
 * `kv-update`), sauf si une écriture locale est en attente.
 */
export function syncAcrossDevices(store: Rehydratable) {
  if (IS_TAURI) return;
  stores.push(store);
  const name = store.persist.getOptions().name;
  void listen<string>("kv-update", (e) => {
    if (e.payload === name && !pending.has(name)) void store.persist.rehydrate();
  });
}
