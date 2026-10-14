import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { CraftOptions } from "@/lib/api";

interface CraftOptionsState extends CraftOptions {
  set: (patch: Partial<CraftOptions>) => void;
}

/** Options de l'analyse d'artisanat, gardées entre deux lancements. */
export const useCraftOptions = create<CraftOptionsState>()(
  persist(
    (set) => ({
      allowExpert: false,
      allowMaster: false,
      includeSpecialist: false,
      set: (patch) => {
        set(patch);
      },
    }),
    { name: "options-artisanat" },
  ),
);
