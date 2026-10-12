import { create } from "zustand";

export type Page = "dashboard" | "scan" | "watchlist" | "search" | "history" | "settings";

interface NavState {
  page: Page;
  setPage: (page: Page) => void;
}

export const useNav = create<NavState>()((set) => ({
  page: "dashboard",
  setPage: (page) => {
    set({ page });
  },
}));
