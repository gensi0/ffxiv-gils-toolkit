import { ConfidenceBadge, ItemIcon, QualityTag } from "@/components/market";
import type { Opportunity, ScanResults, Trip } from "@/lib/api";
import { gil, gilShort } from "@/lib/format";
import { useNav } from "@/store/nav";

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

  return (
    <section className="flex flex-col rounded-md border text-xs">
      <header className="flex items-baseline gap-2 border-b px-3 py-2">
        <span className="text-sm font-semibold">{trip.world}</span>
        <span className="text-muted-foreground">{trip.dc}</span>
        {crossRegion && (
          <span className="text-[11px] text-amber-600 dark:text-amber-400">cross-region</span>
        )}
        <span className="ml-auto text-sm font-semibold tabular-nums">
          {gilShort(trip.profitPerHour)}/h
        </span>
      </header>
      <div className="flex gap-4 px-3 py-1.5 text-[11px] text-muted-foreground">
        <span>
          <span className="text-foreground tabular-nums">{gil(trip.profitPerTrip)}</span> / trip
        </span>
        <span>
          <span className="text-foreground tabular-nums">{trip.slotsUsed}</span>/{INVENTORY_SLOTS}{" "}
          slots
        </span>
        <span>{trip.travelMinutes} min round trip</span>
        <span>{items.length} items</span>
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
              <td className="px-3 text-right">
                <ConfidenceBadge value={o.confidence} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {leftOut > 0 && (
        <p className="border-t px-3 py-1.5 text-[11px] text-muted-foreground">
          {leftOut} more {leftOut === 1 ? "opportunity" : "opportunities"} left out (inventory full
          or same demand as a better pick).
        </p>
      )}
    </section>
  );
}
