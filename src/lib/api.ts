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
  minMargin: number;
  drillDownTopN: number;
  scanIntervalHours: number;
  theme: Theme;
}

export interface AppInfo {
  version: string;
  dataDir: string;
  logDir: string;
}

export interface NetStatus {
  inFlight: number;
  backoffUntilMs: number | null;
  totalRequests: number;
}

export type Quality = "NQ" | "HQ";
export type Confidence = "HOT" | "OK" | "COMPETITIVE" | "SATURATED" | "SLOW";

export interface ItemInfo {
  id: number;
  name: string;
  iconPath: string | null;
  category: string | null;
  stackSize: number;
  levelItem: number | null;
  canBeHq: boolean;
}

export interface SearchHit {
  id: number;
  name: string;
  iconPath: string | null;
  category: string | null;
}

export interface QualitySummary {
  minPrice: number | null;
  unitsAtMin: number;
  listings: number;
  unitsListed: number;
}

export interface WorldRow {
  worldId: number;
  world: string;
  dc: string;
  isHome: boolean;
  nq: QualitySummary;
  hq: QualitySummary;
  uploadMs: number | null;
}

export interface PeriodStats {
  unitsPerDay: number;
  salesPerDay: number;
  average: number | null;
  median: number | null;
}

export interface HomeStats {
  quality: Quality;
  unitsPerDay: number;
  salesPerDay: number;
  median7d: number;
  floorListing: number | null;
  expectedSell: number;
  netSale: number;
  depthUnits: number;
  daysOfSupply: number | null;
}

export interface QualityHomeStats {
  d7: PeriodStats;
  d28: PeriodStats;
  f5: HomeStats | null;
}

export interface ArbitrageRow {
  world: string;
  dc: string;
  worldId: number;
  quality: Quality;
  soldAs: Quality;
  unitsToBuy: number;
  sourceUnitPrice: number;
  sourceCost: number;
  expectedSell: number;
  homeMedian: number;
  homeUnitsPerDay: number;
  daysOfSupply: number | null;
  unitProfit: number;
  profitPerTrip: number;
  dailyProfit: number;
  confidence: Confidence;
  sourceUploadMs: number | null;
  isStale: boolean;
}

export interface ItemDetail {
  item: ItemInfo;
  homeWorld: string;
  sellerTax: number;
  worlds: WorldRow[];
  homeNq: QualityHomeStats;
  homeHq: QualityHomeStats;
  arbitrage: ArbitrageRow[];
  failedDcs: string[];
  fetchedAt: number;
}

export type ScanPhase =
  "preparing" | "prefilter" | "drillDown" | "saving" | "done" | "failed" | "cancelled";

export interface ScanProgress {
  scanId: string;
  phase: ScanPhase;
  done: number;
  total: number;
  etaSecs: number | null;
  itemsFailed: number;
  message: string | null;
}

export interface ScanRun {
  scanId: string;
  startedAt: number;
  completedAt: number | null;
  itemsScanned: number | null;
  itemsFailed: number | null;
  opportunitiesFound: number | null;
  status: "running" | "done" | "failed" | "cancelled";
  error: string | null;
}

export interface Opportunity {
  item: ItemInfo;
  quality: Quality;
  soldAs: Quality;
  sourceWorldId: number;
  sourceWorld: string;
  sourceDc: string;
  crossRegion: boolean;
  sourceUnitPrice: number;
  unitsToBuy: number;
  sourceCost: number;
  expectedSellPrice: number;
  homeMedianPrice: number;
  homeUnitsPerDay: number;
  daysOfSupply: number | null;
  unitProfit: number;
  profitPerTrip: number;
  dailyProfit: number;
  confidence: Confidence;
  sourceUploadMs: number | null;
  isStale: boolean;
}

export interface Trip {
  worldId: number;
  world: string;
  dc: string;
  travelMinutes: number;
  picked: number[];
  slotsUsed: number;
  profitPerTrip: number;
  profitPerHour: number;
}

export interface ScanResults {
  run: ScanRun;
  homeWorld: string;
  opportunities: Opportunity[];
  trips: Trip[];
}

export interface Candidate {
  item: ItemInfo;
  quality: Quality;
  soldAs: Quality;
  sourceDc: string;
  sourceWorld: string;
  sourceMinPrice: number;
  estSellPrice: number;
  estProfit: number;
  estScore: number;
  homeUnitsPerDay: number;
  sourceUploadMs: number | null;
}

/** Events emitted by the backend. */
export const EVENTS = {
  worldsUpdated: "worlds-updated",
  netStatus: "net-status",
  scanProgress: "scan-progress",
} as const;

export const api = {
  listWorlds: () => invoke<World[]>("list_worlds"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  appInfo: () => invoke<AppInfo>("app_info"),
  openLogDir: () => invoke<null>("open_log_dir"),
  netStatus: () => invoke<NetStatus>("net_status"),
  searchItems: (text: string) => invoke<SearchHit[]>("search_items", { text }),
  itemDetail: (itemId: number, refresh: boolean) =>
    invoke<ItemDetail>("item_detail", { itemId, refresh }),
  startScan: () => invoke<string>("start_scan"),
  cancelScan: () => invoke<boolean>("cancel_scan"),
  scanProgress: () => invoke<ScanProgress | null>("scan_progress"),
  listScans: () => invoke<ScanRun[]>("list_scans"),
  scanResults: (scanId: string | null) => invoke<ScanResults | null>("scan_results", { scanId }),
  scanCandidates: (scanId: string) => invoke<Candidate[]>("scan_candidates", { scanId }),
};

/** Tauri rejects with the serialized AppError, which is a plain string. */
export function errorMessage(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return "Unknown error";
}
