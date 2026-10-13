import { create } from "zustand";

export type Page =
  | "dashboard"
  | "scan"
  | "watchlist"
  | "search"
  | "history"
  | "stats"
  | "settings"
  | "item"
  | "monnaies"
  | "pnj"
  | "livraisons"
  | "servants"
  | "desynthese";

interface NavState {
  page: Page;
  /** Item shown on the "item" page. */
  itemId: number | null;
  /** Scan shown on the Scan page; null = latest completed scan. */
  scanId: string | null;
  /** Page to return to from the item page. */
  back: Page;
  setPage: (page: Page) => void;
  openItem: (itemId: number) => void;
  openScan: (scanId: string | null) => void;
  goBack: () => void;
}

export const useNav = create<NavState>()((set, get) => ({
  page: "dashboard",
  itemId: null,
  scanId: null,
  back: "dashboard",
  setPage: (page) => {
    set({ page });
  },
  openItem: (itemId) => {
    const { page } = get();
    set({ page: "item", itemId, back: page === "item" ? get().back : page });
  },
  openScan: (scanId) => {
    set({ page: "scan", scanId });
  },
  goBack: () => {
    set({ page: get().back });
  },
}));
