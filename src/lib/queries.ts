import { useEffect } from "react";
import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { api, EVENTS, type NetStatus, type ScanProgress, type Settings } from "./api";
import { useLive } from "@/store/live";

// The Rust side owns the real cache (SQLite); keep the client cache short.
const SHORT = { staleTime: 30_000 } as const;

export const useWorlds = () =>
  useQuery({ queryKey: ["worlds"], queryFn: api.listWorlds, ...SHORT });

export const useSettings = () =>
  useQuery({ queryKey: ["settings"], queryFn: api.getSettings, ...SHORT });

export const useAppInfo = () =>
  useQuery({ queryKey: ["app-info"], queryFn: api.appInfo, staleTime: Infinity });

export function useSaveSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (s: Settings) => api.saveSettings(s),
    onSuccess: (saved) => {
      qc.setQueryData(["settings"], saved);
      // Results are localized and filtered with settings.
      void qc.invalidateQueries({ queryKey: ["scan"] });
      void qc.invalidateQueries({ queryKey: ["item"] });
    },
  });
}

export const useSearch = (text: string) =>
  useQuery({
    queryKey: ["search", text],
    queryFn: () => api.searchItems(text),
    enabled: text.trim().length >= 2,
    placeholderData: keepPreviousData,
    ...SHORT,
  });

export const useItemDetail = (itemId: number) =>
  useQuery({
    queryKey: ["item", itemId],
    queryFn: () => api.itemDetail(itemId, false),
    ...SHORT,
  });

export const useScanResults = (scanId: string | null) =>
  useQuery({
    queryKey: ["scan", "results", scanId],
    queryFn: () => api.scanResults(scanId),
    ...SHORT,
  });

export const useScanCandidates = (scanId: string | null) =>
  useQuery({
    queryKey: ["scan", "candidates", scanId],
    queryFn: () => api.scanCandidates(scanId ?? ""),
    enabled: scanId !== null,
    ...SHORT,
  });

export const useScans = () =>
  useQuery({ queryKey: ["scan", "runs"], queryFn: api.listScans, ...SHORT });

/**
 * App-wide backend event listeners; mount once (in App). Live state goes to
 * the `useLive` store, and query caches are invalidated when data changes.
 */
export function useBackendEvents() {
  const qc = useQueryClient();
  useEffect(() => {
    const { setScan, setNet } = useLive.getState();
    void api.scanProgress().then((p) => {
      if (p) setScan(p);
    });
    void api.netStatus().then(setNet);
    const subs = [
      listen(EVENTS.worldsUpdated, () => {
        void qc.invalidateQueries({ queryKey: ["worlds"] });
      }),
      listen<NetStatus>(EVENTS.netStatus, (e) => {
        setNet(e.payload);
      }),
      listen<ScanProgress>(EVENTS.scanProgress, (e) => {
        setScan(e.payload);
        if (!isRunning(e.payload)) void qc.invalidateQueries({ queryKey: ["scan"] });
      }),
    ];
    return () => {
      for (const sub of subs) {
        void sub.then((unlisten) => {
          unlisten();
        });
      }
    };
  }, [qc]);
}

export const isRunning = (p: ScanProgress | null) =>
  p !== null && p.phase !== "done" && p.phase !== "failed" && p.phase !== "cancelled";
