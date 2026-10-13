import { useSettings, useWorlds } from "@/lib/queries";
import { usePrefs } from "@/spender/prefs";

/** Serveur utilisé par les outils : choix mémorisé, sinon le serveur principal. */
export function useSpenderWorld(): string {
  const worlds = useWorlds();
  const settings = useSettings();
  const chosen = usePrefs((p) => p.world);
  const home = worlds.data?.find((w) => w.id === settings.data?.homeWorldId);
  return chosen || home?.name || "";
}
