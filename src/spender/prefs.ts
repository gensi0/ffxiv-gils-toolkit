import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import { accountStorage, SKIP_HYDRATION, syncAcrossDevices } from "@/lib/accountStorage";
import type { SortingState } from "@tanstack/react-table";
import type { VentureClass } from "@/spender/api";

export type Tab = "monnaies" | "pnj" | "livraisons" | "servants" | "desynthese";

/** Servant, par classe : niveau, stat des paliers (niveau d'objet ou
 *  perception) et, pour les récolteurs, la récolte qui donne accès aux missions. */
export interface Retainer {
  level: number;
  stat: number;
  gathering?: number;
}

interface Prefs {
  tab: Tab;
  world: string;
  currency: number | null;
  seal: number;
  hideLocked: boolean;
  hideSlow: boolean;
  onlyProfitable: boolean;
  ventureClass: VentureClass;
  retainers: Record<VentureClass, Retainer>;
  sorting: Record<Tab, SortingState>;
  set: (p: Partial<Omit<Prefs, "set" | "setSorting" | "setRetainer">>) => void;
  setSorting: (tab: Tab, s: SortingState) => void;
  setRetainer: (c: VentureClass, r: Partial<Retainer>) => void;
}

/** Préférences conservées entre deux lancements (localStorage). */
export const usePrefs = create<Prefs>()(
  persist(
    (set) => ({
      tab: "monnaies",
      // Vide : serveur principal des paramètres.
      world: "",
      currency: null,
      seal: 20,
      hideLocked: false,
      hideSlow: true,
      onlyProfitable: true,
      ventureClass: "combat",
      retainers: {
        combat: { level: 100, stat: 700 },
        mineur: { level: 100, stat: 4000, gathering: 4000 },
        botaniste: { level: 100, stat: 4000, gathering: 4000 },
        pecheur: { level: 100, stat: 4000, gathering: 4000 },
      },
      sorting: {
        monnaies: [{ id: "gilPerCurrency", desc: true }],
        pnj: [{ id: "dailyProfit", desc: true }],
        livraisons: [{ id: "profit", desc: true }],
        servants: [{ id: "gilPerVenture", desc: true }],
        desynthese: [{ id: "bestGain", desc: true }],
      },
      set: (p) => {
        set(p);
      },
      setSorting: (tab, s) => {
        set((st) => ({ sorting: { ...st.sorting, [tab]: s } }));
      },
      setRetainer: (c, r) => {
        set((st) => ({ retainers: { ...st.retainers, [c]: { ...st.retainers[c], ...r } } }));
      },
    }),
    {
      name: "gils-toolkit-depense",
      storage: createJSONStorage(() => accountStorage),
      skipHydration: SKIP_HYDRATION,
      version: 1,
    },
  ),
);

syncAcrossDevices(usePrefs);
