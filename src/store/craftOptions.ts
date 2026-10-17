import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import { accountStorage, SKIP_HYDRATION, syncAcrossDevices } from "@/lib/accountStorage";
import type { CraftOptions } from "@/lib/api";

export type CraftSortKey =
  | "item"
  | "job"
  | "unitCost"
  | "sellPrice"
  | "unitProfit"
  | "margin"
  | "unitsPerDay"
  | "profitPerDay";

interface CraftOptionsState extends CraftOptions {
  /** Tri du tableau (clic sur un en-tête). */
  sort: { key: CraftSortKey; desc: boolean };
  set: (patch: Partial<CraftOptions & { sort: { key: CraftSortKey; desc: boolean } }>) => void;
}

/** Options de l'analyse d'artisanat et tri, gardés entre deux lancements. */
export const useCraftOptions = create<CraftOptionsState>()(
  persist(
    (set) => ({
      allowExpert: false,
      allowMaster: false,
      includeSpecialist: false,
      gatherMyself: false,
      sort: { key: "profitPerDay", desc: true },
      set: (patch) => {
        set(patch);
      },
    }),
    {
      name: "options-artisanat",
      storage: createJSONStorage(() => accountStorage),
      skipHydration: SKIP_HYDRATION,
    },
  ),
);

syncAcrossDevices(useCraftOptions);
