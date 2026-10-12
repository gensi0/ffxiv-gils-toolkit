import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { EmptyState, PageHeader } from "@/components/Page";
import { Age, ConfidenceBadge, ItemIcon, QualityTag } from "@/components/market";
import { ScanButton, ScanProgressBar } from "@/components/ScanControls";
import { errorMessage, type ScanResults } from "@/lib/api";
import { gil, gilShort } from "@/lib/format";
import { useScanResults } from "@/lib/queries";
import { useNav } from "@/store/nav";

export function DashboardPage() {
  const results = useScanResults(null);
  return (
    <>
      <PageHeader title="Dashboard">
        <ScanButton />
      </PageHeader>
      <ScanProgressBar />
      {results.isError ? (
        <div className="flex items-center gap-3 p-4 text-xs text-destructive">
          {errorMessage(results.error)}
          <Button variant="outline" size="sm" onClick={() => void results.refetch()}>
            Retry
          </Button>
        </div>
      ) : results.isPending ? (
        <div className="grid gap-4 p-4">
          <Skeleton className="h-20" />
          <Skeleton className="h-64" />
        </div>
      ) : results.data === null ? (
        <EmptyState
          message="No scan yet. A full scan takes a few minutes."
          action={<ScanButton />}
        />
      ) : (
        <Summary r={results.data} />
      )}
    </>
  );
}

function Tile({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="rounded-md border px-3 py-2">
      <div className="text-[11px] text-muted-foreground">{label}</div>
      <div className="text-base font-semibold tabular-nums">{children}</div>
    </div>
  );
}

function Summary({ r }: { r: ScanResults }) {
  const openItem = useNav((s) => s.openItem);
  const openScan = useNav((s) => s.openScan);
  // Best source per item: the same item from five worlds is one decision.
  const seen = new Set<number>();
  const top = r.opportunities
    .filter((o) => !seen.has(o.item.id) && seen.add(o.item.id))
    .slice(0, 10);
  const trips = r.trips.slice(0, 3);
  const totalDaily = r.opportunities.reduce((sum, o) => sum + o.dailyProfit, 0);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto p-4">
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <Tile label="Last scan">
          <Age ms={r.run.startedAt} staleHours={12} />
        </Tile>
        <Tile label="Opportunities">{r.opportunities.length}</Tile>
        <Tile label="Expected gil / day (all)">{gilShort(totalDaily)}</Tile>
        <Tile label="Best trip / hour">{trips[0] ? gilShort(trips[0].profitPerHour) : "—"}</Tile>
      </div>

      {trips.length > 0 && (
        <section className="flex flex-col gap-2">
          <h2 className="text-[11px] font-semibold tracking-wider text-muted-foreground uppercase">
            Best source worlds
          </h2>
          <div className="grid gap-3 md:grid-cols-3">
            {trips.map((t) => (
              <div key={t.worldId} className="rounded-md border p-3 text-xs">
                <div className="flex items-baseline justify-between">
                  <span className="font-semibold">
                    {t.world} <span className="font-normal text-muted-foreground">· {t.dc}</span>
                  </span>
                  <span className="text-sm font-semibold tabular-nums">
                    {gilShort(t.profitPerHour)}/h
                  </span>
                </div>
                <div className="mt-1 flex gap-3 text-[11px] text-muted-foreground">
                  <span>{gil(t.profitPerTrip)} / trip</span>
                  <span>{t.picked.length} items</span>
                  <span>{t.slotsUsed}/140 slots</span>
                  <span>{t.travelMinutes} min</span>
                </div>
              </div>
            ))}
          </div>
        </section>
      )}

      <section className="flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <h2 className="text-[11px] font-semibold tracking-wider text-muted-foreground uppercase">
            Top opportunities
          </h2>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              openScan(null);
            }}
          >
            All results
          </Button>
        </div>
        {top.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            The last scan found nothing above your thresholds.
          </p>
        ) : (
          <table className="w-full text-xs">
            <thead className="text-muted-foreground">
              <tr className="border-b">
                <th className="h-7 px-2 text-left font-medium">Item</th>
                <th className="px-2 text-left font-medium">Source</th>
                <th className="px-2 text-right font-medium">Units</th>
                <th className="px-2 text-right font-medium">Profit / trip</th>
                <th className="px-2 text-right font-medium">Gil / day</th>
                <th className="px-2 text-left font-medium">Confidence</th>
              </tr>
            </thead>
            <tbody>
              {top.map((o) => (
                <tr
                  key={`${o.item.id}-${o.quality}-${o.sourceWorldId}`}
                  className="cursor-pointer border-b border-border/50 hover:bg-muted/50"
                  onClick={() => {
                    openItem(o.item.id);
                  }}
                >
                  <td className="h-8 px-2">
                    <span className="flex items-center gap-2">
                      <ItemIcon path={o.item.iconPath} className="size-5" />
                      <span className="max-w-72 truncate">{o.item.name}</span>
                      <QualityTag quality={o.quality} soldAs={o.soldAs} />
                    </span>
                  </td>
                  <td className="px-2">
                    {o.sourceWorld} <span className="text-muted-foreground">· {o.sourceDc}</span>
                  </td>
                  <td className="px-2 text-right tabular-nums">{gil(o.unitsToBuy)}</td>
                  <td className="px-2 text-right tabular-nums">{gil(o.profitPerTrip)}</td>
                  <td className="px-2 text-right font-medium tabular-nums">{gil(o.dailyProfit)}</td>
                  <td className="px-2">
                    <ConfidenceBadge value={o.confidence} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}
