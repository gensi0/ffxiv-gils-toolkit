import type { Lot, Opportunity } from "@/lib/api";

/** Taxe payée par l'acheteur en plus du prix affiché. */
const BUYER_TAX = 0.05;

/** Prix unitaire moyen payé (hors taxe), toutes annonces confondues. */
export function averagePrice(lots: Lot[], units: number, costWithTax: number): number {
  const spent = lots.reduce((t, l) => t + l.price * l.quantity, 0);
  if (lots.length > 0 && units > 0) return spent / units;
  return units > 0 ? costWithTax / (1 + BUYER_TAX) / units : 0;
}

export const oppAverage = (o: Opportunity) => averagePrice(o.lots, o.unitsToBuy, o.sourceCost);

/** Bénéfice ÷ coût : 0,5 = la mise rapporte 50 % de plus. */
export const margin = (profit: number, cost: number) => (cost > 0 ? profit / cost : 0);
