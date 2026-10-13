// Commandes Tauri des outils de dépense (src-tauri/src/spender.rs).
import { invoke } from "@tauri-apps/api/core";

export type Category =
  "assignats" | "memoquartz" | "grandeCompagnie" | "tribusAlliees" | "jcj" | "chasse" | "autres";

export const CATEGORY_LABEL: Record<Category, string> = {
  assignats: "Assignats",
  memoquartz: "Mémoquartz",
  grandeCompagnie: "Grandes compagnies",
  tribusAlliees: "Tribus alliées",
  jcj: "JcJ",
  chasse: "Chasse",
  autres: "Autres",
};

export interface Currency {
  itemId: number;
  name: string;
  icon: string | null;
  category: Category;
  itemCount: number;
}

export interface Unlock {
  reason: string | null;
}

export interface PricedOffer {
  item: number;
  name: string;
  icon: string | null;
  quantity: number;
  cost: number;
  hq: boolean;
  shops: string[];
  vendors: string[];
  unlock: Unlock | null;
  minListing: number | null;
  averageSale: number | null;
  lastSaleMs: number | null;
  dailyVelocity: number | null;
  unitPrice: number | null;
  gilPerCurrency: number | null;
}

export interface PricedGilOffer {
  item: number;
  name: string;
  icon: string | null;
  npcPrice: number;
  vendors: string[];
  unlock: Unlock | null;
  unitPrice: number | null;
  minListing: number | null;
  averageSale: number | null;
  dailyVelocity: number | null;
  profit: number | null;
  margin: number | null;
  dailyProfit: number | null;
}

export interface PricedDelivery {
  item: number;
  name: string;
  icon: string | null;
  itemLevel: number;
  seals: number;
  buyPrice: number | null;
  dailyVelocity: number | null;
  sealsValue: number | null;
  profit: number | null;
  gilPerSeal: number | null;
}

export interface CurrencyValue {
  gil: number;
  itemName: string;
  cost: number;
}

export interface Deliveries {
  sealValue: CurrencyValue | null;
  rows: PricedDelivery[];
}

export type VentureClass = "combat" | "mineur" | "botaniste" | "pecheur";

export interface VentureClassInfo {
  id: VentureClass;
  label: string;
  /** Job que doit avoir le servant. */
  job: string;
  /** Stat qui détermine le palier (quantité rapportée). */
  tierStat: string;
  /** Stat minimale pour accéder à la mission. */
  requiredStat: string;
}

export const DEFAULT_CLASS: VentureClassInfo = {
  id: "combat",
  label: "Combat",
  job: "un job de combat ou de mage",
  tierStat: "Niveau d'objet moyen",
  requiredStat: "Niveau d'objet moyen",
};

export const VENTURE_CLASSES: VentureClassInfo[] = [
  DEFAULT_CLASS,

  {
    id: "mineur",
    label: "Mineur",
    job: "Mineur",
    tierStat: "Perception",
    requiredStat: "Récolte",
  },
  {
    id: "botaniste",
    label: "Botaniste",
    job: "Botaniste",
    tierStat: "Perception",
    requiredStat: "Récolte",
  },
  {
    id: "pecheur",
    label: "Pêcheur",
    job: "Pêcheur",
    tierStat: "Perception",
    requiredStat: "Récolte",
  },
];

export interface PricedVenture {
  item: number;
  name: string;
  icon: string | null;
  class: VentureClass;
  retainerLevel: number;
  requiredStat: number;
  thresholds: number[];
  quantities: number[];
  unitPrice: number | null;
  minListing: number | null;
  averageSale: number | null;
  dailyVelocity: number | null;
}

export interface DesynthResult {
  item: number;
  name: string;
  icon: string | null;
  unitPrice: number | null;
  dailyVelocity: number | null;
}

export interface PricedDesynth {
  item: number;
  name: string;
  icon: string | null;
  buyPrice: number | null;
  dailyVelocity: number | null;
  results: DesynthResult[];
  bestResult: number | null;
  bestGain: number | null;
}

/** Sceaux par grande compagnie (identifiants d'objet). */
export const GRAND_COMPANIES = [
  { seal: 20, name: "Maelstrom" },
  { seal: 21, name: "Deux Vipères" },
  { seal: 22, name: "Immortels" },
] as const;

export interface Status {
  ready: boolean;
  builtAtMs: number | null;
}

export interface BuildProgress {
  step: string;
  done: number;
  total: number;
}

export const EVENTS = { buildProgress: "build-progress" } as const;

export const spenderApi = {
  datasetStatus: () => invoke<Status>("spender_dataset_status"),
  buildDataset: () => invoke<Status>("spender_build_dataset"),
  currencies: () => invoke<Currency[]>("spender_currencies"),
  offers: (currency: number, world: string, refresh: boolean) =>
    invoke<PricedOffer[]>("spender_offers", { currency, world, refresh }),
  gilOffers: (world: string, refresh: boolean) =>
    invoke<PricedGilOffer[]>("spender_gil_offers", { world, refresh }),
  deliveries: (world: string, seal: number, refresh: boolean) =>
    invoke<Deliveries>("spender_deliveries", { world, seal, refresh }),
  ventures: (world: string, refresh: boolean) =>
    invoke<PricedVenture[]>("spender_ventures", { world, refresh }),
  desynths: (world: string, refresh: boolean) =>
    invoke<PricedDesynth[]>("spender_desynths", { world, refresh }),
};
