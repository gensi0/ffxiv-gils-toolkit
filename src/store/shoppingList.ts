import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { Lot, Opportunity, Quality } from "@/lib/api";

/** Ligne de la liste de courses : instantané de l'opportunité au moment de l'ajout. */
export interface ShoppingItem {
  key: string;
  itemId: number;
  name: string;
  iconPath: string | null;
  quality: Quality;
  soldAs: Quality;
  worldId: number;
  world: string;
  dc: string;
  crossRegion: boolean;
  /** Prix de l'annonce la moins chère (anciennes entrées) — voir `lots`. */
  unitPrice: number;
  /** Absents pour les entrées ajoutées avant le détail des annonces. */
  maxUnitPrice?: number | null;
  lots?: Lot[];
  units: number;
  cost: number;
  sellPrice: number;
  profit: number;
  addedAt: number;
  bought: boolean;
  /** Achat noté dans le journal des gains. */
  purchaseId?: number;
}

export const shoppingKey = (o: Opportunity) =>
  `${String(o.item.id)}-${o.quality}-${String(o.sourceWorldId)}`;

function fromOpportunity(o: Opportunity): ShoppingItem {
  return {
    key: shoppingKey(o),
    itemId: o.item.id,
    name: o.item.name,
    iconPath: o.item.iconPath,
    quality: o.quality,
    soldAs: o.soldAs,
    worldId: o.sourceWorldId,
    world: o.sourceWorld,
    dc: o.sourceDc,
    crossRegion: o.crossRegion,
    unitPrice: o.sourceUnitPrice,
    maxUnitPrice: o.maxUnitPrice,
    lots: o.lots,
    units: o.unitsToBuy,
    cost: o.sourceCost,
    sellPrice: o.expectedSellPrice,
    profit: o.profitPerTrip,
    addedAt: Date.now(),
    bought: false,
  };
}

interface ShoppingState {
  items: ShoppingItem[];
  add: (opps: Opportunity[]) => void;
  remove: (key: string) => void;
  toggle: (o: Opportunity) => void;
  setBought: (key: string, bought: boolean, purchaseId?: number) => void;
  clearBought: () => void;
  clear: () => void;
}

/** Liste de courses, gardée entre deux lancements (localStorage). */
export const useShoppingList = create<ShoppingState>()(
  persist(
    (set, get) => ({
      items: [],
      add: (opps) => {
        const have = new Set(get().items.map((i) => i.key));
        const fresh = opps.filter((o) => !have.has(shoppingKey(o))).map(fromOpportunity);
        set({ items: [...get().items, ...fresh] });
      },
      remove: (key) => {
        set({ items: get().items.filter((i) => i.key !== key) });
      },
      toggle: (o) => {
        const key = shoppingKey(o);
        if (get().items.some((i) => i.key === key)) get().remove(key);
        else get().add([o]);
      },
      setBought: (key, bought, purchaseId) => {
        set({
          items: get().items.map((i) =>
            i.key === key ? { ...i, bought, purchaseId: bought ? purchaseId : undefined } : i,
          ),
        });
      },
      clearBought: () => {
        set({ items: get().items.filter((i) => !i.bought) });
      },
      clear: () => {
        set({ items: [] });
      },
    }),
    { name: "liste-de-courses" },
  ),
);
