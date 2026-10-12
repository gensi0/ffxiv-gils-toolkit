// Typed wrappers around the Rust Tauri commands (src-tauri/src/commands.rs).
import { invoke } from "@tauri-apps/api/core";

export interface World {
  id: number;
  name: string;
  dc: string;
  region: string;
}

export const RETAINER_CITIES = [
  "Limsa Lominsa",
  "Gridania",
  "Ul'dah",
  "Ishgard",
  "Kugane",
  "Crystarium",
  "Old Sharlayan",
  "Tuliyollal",
] as const;
export type RetainerCity = (typeof RETAINER_CITIES)[number];

export const LANGUAGES = ["en", "fr", "de", "ja"] as const;
export type Language = (typeof LANGUAGES)[number];

export type Theme = "dark" | "light";

export interface Settings {
  homeWorldId: number | null;
  retainerCity: RetainerCity;
  language: Language;
  extraRegions: string[];
  travelMinutesInRegion: number;
  travelMinutesCrossRegion: number;
  minProfitPerTrip: number;
  minUnitsPerDay: number;
  sellHorizonDays: number;
  maxDataAgeHours: number;
  theme: Theme;
}

export interface AppInfo {
  version: string;
  dataDir: string;
  logDir: string;
}

/** Event emitted by the backend when the stored world list changed. */
export const EVENT_WORLDS_UPDATED = "worlds-updated";

export const api = {
  listWorlds: () => invoke<World[]>("list_worlds"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  appInfo: () => invoke<AppInfo>("app_info"),
  openLogDir: () => invoke<null>("open_log_dir"),
};

/** Tauri rejects with the serialized AppError, which is a plain string. */
export function errorMessage(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return "Unknown error";
}
