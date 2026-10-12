import { create } from "zustand";
import type { SortingState } from "@tanstack/react-table";

interface ScanFiltersState {
  category: string[];
  selRegions: string[];
  selDcs: string[];
  selWorlds: string[];
  selConfidence: string[];
  hqOnly: boolean;
  minProfit: string;
  sorting: SortingState;
  setCategory: (v: string[]) => void;
  setSelRegions: (v: string[]) => void;
  setSelDcs: (v: string[]) => void;
  setSelWorlds: (v: string[]) => void;
  setSelConfidence: (v: string[]) => void;
  setHqOnly: (v: boolean) => void;
  setMinProfit: (v: string) => void;
  setSorting: (v: SortingState) => void;
}

export const useScanFilters = create<ScanFiltersState>()((set) => ({
  category: [],
  selRegions: [],
  selDcs: [],
  selWorlds: [],
  selConfidence: [],
  hqOnly: false,
  minProfit: "",
  sorting: [{ id: "dailyProfit", desc: true }],
  setCategory: (v) => set({ category: v }),
  setSelRegions: (v) => set({ selRegions: v }),
  setSelDcs: (v) => set({ selDcs: v }),
  setSelWorlds: (v) => set({ selWorlds: v }),
  setSelConfidence: (v) => set({ selConfidence: v }),
  setHqOnly: (v) => set({ hqOnly: v }),
  setMinProfit: (v) => set({ minProfit: v }),
  setSorting: (v) => set({ sorting: v }),
}));
