import { ConfidenceBadge, ItemIcon, QualityTag } from "@/components/market";
import type { Opportunity, ScanResults, Trip } from "@/lib/api";
import { gil, gilShort } from "@/lib/format";
import { ShoppingCartIcon } from "lucide-react";
import { CartButton } from "@/components/CartButton";
import { useNav } from "@/store/nav";
import { shoppingKey, useShoppingList } from "@/store/shoppingList";

const INVENTORY_SLOTS = 140;

/** F7: one card per source world, the way a trip is actually played. */
export function TripCards({ r }: { r: ScanResults }) {
  return (
    <div className="min-h-0 flex-1 overflow-y-auto p-4">
      <div className="grid gap-3 lg:grid-cols-2 2xl:grid-cols-3">
        {r.trips.map((t) => (
          <TripCard key={t.worldId} trip={t} opportunities={r.opportunities} />
        ))}
      </div>
    </div>
  );
}

function TripCard({ trip, opportunities }: { trip: Trip; opportunities: Opportunity[] }) {
  const openItem = useNav((s) => s.openItem);
  const picked = new Set(trip.picked);
  const items = trip.picked
    .map((i) => opportunities[i])
    .filter((o): o is Opportunity => o !== undefined)
    .sort((a, b) => b.profitPerTrip - a.profitPerTrip);
  const leftOut = opportunities.filter(
    (o, i) => o.sourceWorldId === trip.worldId && !picked.has(i),
  ).length;
  const crossRegion = items[0]?.crossRegion ?? false;
  const addAll = useShoppingList((s) => s.add);
  const allIn = useShoppingList((s) => {
    const keys = new Set(s.items.map((i) => i.key));
    return items.every((o) => keys.has(shoppingKey(o)));
  });

  return (
    <section className="flex flex-col rounded-md border text-xs">
      <header className="flex items-baseline gap-2 border-b px-3 py-2">
        <span className="text-sm font-semibold">{trip.world}</span>
        <span className="text-muted-foreground">{trip.dc}</span>
        {crossRegion && (
          <span className="text-[11px] text-amber-600 dark:text-amber-400">inter-région</span>
        )}
        <span className="ml-auto text-sm font-semibold tabular-nums">
          {gilShort(trip.profitPerHour)}/h
        </span>
      </header>
      <div className="flex gap-4 px-3 py-1.5 text-[11px] text-muted-foreground">
        <span>
          <span className="text-foreground tabular-nums">{gil(trip.profitPerTrip)}</span> / trajet
        </span>
        <span>
          <span className="text-foreground tabular-nums">{trip.slotsUsed}</span>/{INVENTORY_SLOTS}{" "}
          emplacements
        </span>
        <span>{trip.travelMinutes} min aller-retour</span>
        <span>{items.length} objets</span>
        <button
          type="button"
          disabled={allIn}
          onClick={() => {
            addAll(items);
          }}
          className="ml-auto inline-flex items-center gap-1 text-emerald-600 hover:underline disabled:text-muted-foreground disabled:no-underline dark:text-emerald-400"
        >
          <ShoppingCartIcon className="size-3" />
          {allIn ? "Tout est dans la liste" : "Tout ajouter à la liste"}
        </button>
      </div>
      <table className="w-full">
        <tbody>
          {items.map((o) => (
            <tr
              key={`${o.item.id}-${o.quality}`}
              className="cursor-pointer border-t border-border/50 hover:bg-muted/50"
              onClick={() => {
                openItem(o.item.id);
              }}
            >
              <td className="h-7 px-3">
                <span className="flex items-center gap-2">
                  <ItemIcon path={o.item.iconPath} className="size-4" />
                  <span className="max-w-52 truncate">{o.item.name}</span>
                  <QualityTag quality={o.quality} soldAs={o.soldAs} />
                </span>
              </td>
              <td className="px-2 text-right text-muted-foreground tabular-nums">
                {gil(o.unitsToBuy)} × {gil(o.sourceUnitPrice)}
              </td>
              <td className="px-2 text-right tabular-nums">{gil(o.profitPerTrip)}</td>
              <td className="px-1 text-right">
                <ConfidenceBadge value={o.confidence} />
              </td>
              <td className="w-8 pr-2">
                <CartButton o={o} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {leftOut > 0 && (
        <p className="border-t px-3 py-1.5 text-[11px] text-muted-foreground">
          {leftOut} {leftOut === 1 ? "autre opportunité écartée" : "autres opportunités écartées"}{" "}
          (inventaire plein ou même demande qu'un meilleur choix).
        </p>
      )}
    </section>
  );
}
