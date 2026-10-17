import { useSyncExternalStore } from "react";

/** Téléphone (Android, iOS) : l'app passe en mode compagnon. */
export const IS_MOBILE = /Android|iPhone|iPad/i.test(navigator.userAgent);

const query = window.matchMedia("(max-width: 700px)");
const subscribe = (cb: () => void) => {
  query.addEventListener("change", cb);
  return () => {
    query.removeEventListener("change", cb);
  };
};

/** Affichage compact : téléphone, ou fenêtre très étroite sur ordinateur. */
export function useCompact(): boolean {
  const narrow = useSyncExternalStore(subscribe, () => query.matches);
  return IS_MOBILE || narrow;
}
