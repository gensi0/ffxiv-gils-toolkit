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
  notificationsEnabled: boolean;
  watchNotifyProfit: number;
  theme: Theme;
  /** Noms de mes servants, pour reconnaître mes annonces. */
  retainerNames: string[];
  /** Alerte quand une de mes annonces sort de ce rang. */
  salesTopRank: number;
  /** Niveaux des métiers d'artisan, ordre de CRAFT_JOBS ; vide = non renseignés. */
  crafterLevels: number[];
  /** Lien ou identifiant Lodestone. */
  lodestone: string;
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
  maxUnitPrice: number;
  lots: Lot[];
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

export interface SalePoint {
  t: number;
  price: number;
  quantity: number;
  hq: boolean;
}

export interface ItemDetail {
  item: ItemInfo;
  homeWorld: string;
  sellerTax: number;
  worlds: WorldRow[];
  homeNq: QualityHomeStats;
  homeHq: QualityHomeStats;
  arbitrage: ArbitrageRow[];
  sales: SalePoint[];
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

/** Annonce à acheter : prix unitaire affiché (hors taxe) et quantité. */
// ---------- artisanat ----------

/** Métiers d'artisan, dans l'ordre du jeu et du Lodestone. */
export const CRAFT_JOBS = [
  "Menuisier",
  "Forgeron",
  "Armurier",
  "Orfèvre",
  "Tanneur",
  "Couturier",
  "Alchimiste",
  "Cuisinier",
] as const;

export type CraftSource =
  | { kind: "buy"; world: string; worldId: number; dc: string }
  | { kind: "craft"; recipeId: number; job: number; level: number }
  | { kind: "missing" };

export interface CraftNode {
  item: number;
  quantity: number;
  unitCost: number;
  source: CraftSource;
  children: CraftNode[];
}

export interface CraftRow {
  recipeId: number;
  item: ItemInfo;
  job: number;
  level: number;
  amount: number;
  specialist: boolean;
  masterBook: boolean;
  expert: boolean;
  sellPrice: number;
  unitsPerDay: number;
  unitCost: number;
  unitProfit: number;
  margin: number;
  profitPerDay: number;
  suggestedCrafts: number;
  tree: CraftNode;
}

export interface CraftResults {
  computedAtMs: number;
  taxRate: number;
  recipesConsidered: number;
  rows: CraftRow[];
  items: Record<string, ItemInfo>;
}

export interface CraftOptions {
  allowExpert: boolean;
  allowMaster: boolean;
  includeSpecialist: boolean;
}

export interface LodestoneJobs {
  character: string;
  levels: number[];
}

// ---------- mes ventes et journal ----------

export type ListingStatus = "active" | "pending" | "sold" | "removed";

export interface MyListing {
  listingId: string;
  item: ItemInfo;
  hq: boolean;
  price: number;
  quantity: number;
  retainer: string;
  firstSeen: number;
  lastSeen: number;
  status: ListingStatus;
  /** Nouveau prix saisi après une modification en jeu, pas encore vu sur Universalis. */
  expectedPrice: number | null;
  /** Prix concurrent le plus bas quand on est passé sous mon prix. */
  undercutPrice: number | null;
  suggestedPrice: number | null;
  /** 1 = la moins chère parmi les autres vendeurs de même qualité. */
  rank: number | null;
  listingCount: number | null;
  cheapestOther: number | null;
  /** Coût moyen d'achat noté (liste de courses), si connu. */
  unitCost: number | null;
}

export interface StockLine {
  item: ItemInfo;
  hq: boolean;
  remaining: number;
  avgCost: number;
}

export interface VentesOverview {
  configured: boolean;
  lastPollMs: number | null;
  lastDiscoveryMs: number | null;
  discovering: boolean;
  taxRate: number;
  topRank: number;
  listings: MyListing[];
  pending: MyListing[];
  stock: StockLine[];
  tracked: ItemInfo[];
}

export interface Purchase {
  id: number;
  item: ItemInfo;
  hq: boolean;
  quantity: number;
  remaining: number;
  unitCost: number;
  boughtAt: number;
  sourceWorld: string | null;
}

export interface JournalSale {
  id: number;
  item: ItemInfo;
  hq: boolean;
  quantity: number;
  unitPrice: number;
  taxRate: number;
  soldAt: number;
  cost: number;
  costUnits: number;
  source: "auto" | "manual";
}

export interface Journal {
  purchases: Purchase[];
  sales: JournalSale[];
}

export interface Lot {
  price: number;
  quantity: number;
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
  /** Prix unitaire de l'annonce la plus chère à acheter (absent sur les anciens scans). */
  maxUnitPrice: number | null;
  /** Annonces à acheter, de la moins chère à la plus chère (vide sur les anciens scans). */
  lots: Lot[];
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

// ---------- stats ----------

export interface WorldStat {
  worldId: number;
  worldName: string;
  dc: string;
  totalOpps: number;
  totalDailyProfit: number;
  avgUnitProfit: number;
  avgGapPct: number;
  bestConfidence: Confidence;
}

export interface DcStat {
  dc: string;
  worldCount: number;
  totalOpps: number;
  totalDailyProfit: number;
  avgGapPct: number;
}

export interface RegionStat {
  region: string;
  dcCount: number;
  worldCount: number;
  totalOpps: number;
  totalDailyProfit: number;
  avgGapPct: number;
}

export interface ConfidenceBucket {
  label: Confidence;
  count: number;
  totalDailyProfit: number;
}

export interface RatioBucket {
  lower: number;
  label: string;
  count: number;
}

export interface MarketStats {
  totalScans: number;
  totalOpportunities: number;
  bestScanDailyProfit: number;
  avgOppsPerScan: number;
  worldRankings: WorldStat[];
  dcRankings: DcStat[];
  regionRankings: RegionStat[];
  confidenceDist: ConfidenceBucket[];
  profitRatioDist: RatioBucket[];
}

export type WatchChange = "new" | "up" | "down";

export interface WatchItem {
  itemId: number;
  item: ItemInfo | null;
  addedAt: number;
  notifyAboveProfit: number | null;
  loaded: boolean;
  error: string | null;
  best: ArbitrageRow | null;
  homeMinNq: number | null;
  homeMinHq: number | null;
  change: WatchChange | null;
  changedAt: number | null;
  lastEventMs: number | null;
}

export type WsStatus =
  | { state: "idle" }
  | { state: "connecting" }
  | { state: "connected"; subscriptions: number }
  | { state: "disconnected"; retryAtMs: number; error: string };

/** Events emitted by the backend. */
export const EVENTS = {
  worldsUpdated: "worlds-updated",
  netStatus: "net-status",
  scanProgress: "scan-progress",
  watchUpdate: "watch-update",
  wsStatus: "ws-status",
  ventesUpdate: "ventes-update",
  craftProgress: "craft-progress",
} as const;

export const api = {
  listWorlds: () => invoke<World[]>("list_worlds"),
  craftAnalyze: (options: CraftOptions) => invoke<CraftResults>("craft_analyze", { options }),
  craftLast: () => invoke<CraftResults | null>("craft_last"),
  craftLodestone: (input: string) => invoke<LodestoneJobs>("craft_lodestone", { input }),
  ventesOverview: () => invoke<VentesOverview>("ventes_overview"),
  ventesRefresh: () => invoke<VentesOverview>("ventes_refresh"),
  ventesDiscover: () => invoke<VentesOverview>("ventes_discover"),
  ventesSetPrice: (listingId: string, price: number) =>
    invoke<null>("ventes_set_price", { listingId, price }),
  ventesClose: (listingId: string, sold: boolean, price: number | null) =>
    invoke<null>("ventes_close", { listingId, sold, price }),
  ventesTrack: (itemId: number, on: boolean) => invoke<null>("ventes_track", { itemId, on }),
  journalList: (sinceMs: number) => invoke<Journal>("journal_list", { sinceMs }),
  journalAddPurchase: (p: {
    itemId: number;
    hq: boolean;
    quantity: number;
    unitCost: number;
    sourceWorld: string | null;
  }) => invoke<number>("journal_add_purchase", p),
  journalDeletePurchase: (id: number) => invoke<null>("journal_delete_purchase", { id }),
  journalAddSale: (s: { itemId: number; hq: boolean; quantity: number; unitPrice: number }) =>
    invoke<null>("journal_add_sale", s),
  journalDeleteSale: (id: number) => invoke<null>("journal_delete_sale", { id }),
  /** Taxe de vente de la semaine (en %) par cité, pour un serveur. */
  taxRates: (worldId: number) => invoke<Record<string, number>>("tax_rates", { worldId }),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  appInfo: () => invoke<AppInfo>("app_info"),
  openLogDir: () => invoke<null>("open_log_dir"),
  netStatus: () => invoke<NetStatus>("net_status"),
  searchItems: (text: string) => invoke<SearchHit[]>("search_items", { text }),
  itemDetail: (itemId: number, refresh: boolean) =>
    invoke<ItemDetail>("item_detail", { itemId, refresh }),
  startScan: () => invoke<string>("start_scan"),
  retryFailedScan: (scanId: string) => invoke<null>("retry_failed_scan", { scanId }),
  cancelScan: () => invoke<boolean>("cancel_scan"),
  scanProgress: () => invoke<ScanProgress | null>("scan_progress"),
  listScans: () => invoke<ScanRun[]>("list_scans"),
  scanResults: (scanId: string | null) => invoke<ScanResults | null>("scan_results", { scanId }),
  scanCandidates: (scanId: string) => invoke<Candidate[]>("scan_candidates", { scanId }),
  watchlist: () => invoke<WatchItem[]>("watchlist_list"),
  watchAdd: (itemId: number) => invoke<null>("watchlist_add", { itemId }),
  watchRemove: (itemId: number) => invoke<null>("watchlist_remove", { itemId }),
  watchSetThreshold: (itemId: number, profit: number | null) =>
    invoke<null>("watchlist_set_threshold", { itemId, profit }),
  wsStatus: () => invoke<WsStatus>("ws_status"),
  marketStats: () => invoke<MarketStats>("market_stats"),
};

/** Tauri rejects with the serialized AppError, which is a plain string. */
export function errorMessage(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return "Unknown error";
}
