import { create } from "zustand";
import type { NetStatus, ScanProgress } from "@/lib/api";

/** Live backend state pushed through Tauri events (see useBackendEvents). */
interface LiveState {
  scan: ScanProgress | null;
  net: NetStatus | null;
  setScan: (scan: ScanProgress) => void;
  setNet: (net: NetStatus) => void;
}

export const useLive = create<LiveState>()((set) => ({
  scan: null,
  net: null,
  setScan: (scan) => {
    set({ scan });
  },
  setNet: (net) => {
    set({ net });
  },
}));
