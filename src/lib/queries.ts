import { useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { api, EVENT_WORLDS_UPDATED, type Settings } from "./api";

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
    },
  });
}

/** Refetches the world list when the backend's background refresh lands. */
export function useWorldsUpdatedListener() {
  const qc = useQueryClient();
  useEffect(() => {
    const unlisten = listen(EVENT_WORLDS_UPDATED, () => {
      void qc.invalidateQueries({ queryKey: ["worlds"] });
    });
    return () => {
      void unlisten.then((fn) => {
        fn();
      });
    };
  }, [qc]);
}
