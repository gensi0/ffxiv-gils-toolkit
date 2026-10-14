import { useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { EmptyState, PageHeader } from "@/components/Page";
import { Age, ConfidenceBadge, ItemIcon, QualityTag } from "@/components/market";
import { ScanButton, ScanProgressBar } from "@/components/ScanControls";
import { api, errorMessage, type ScanResults } from "@/lib/api";
import { gil, gilShort } from "@/lib/format";
import { useJournal, useScanResults, useVentes } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { useShoppingList } from "@/store/shoppingList";
import { useNav } from "@/store/nav";

export function DashboardPage() {
  const results = useScanResults(null);
  return (
    <>
      <PageHeader title="Tableau de bord">
        <ScanButton />
      </PageHeader>
      <ScanProgressBar />
      <Activity />
      {results.isError ? (
        <div className="flex items-center gap-3 p-4 text-xs text-destructive">
          {errorMessage(results.error)}
          <Button variant="outline" size="sm" onClick={() => void results.refetch()}>
            Réessayer
          </Button>
        </div>
      ) : results.isPending ? (
        <div className="grid gap-4 p-4">
          <Skeleton className="h-20" />
          <Skeleton className="h-64" />
        </div>
      ) : results.data === null ? (
        <EmptyState
          message="Aucun scan pour l'instant. Un scan complet prend quelques minutes."
          action={<ScanButton />}
        />
      ) : (
        <Summary r={results.data} />
      )}
    </>
  );
}

const WEEK = 7 * 24 * 3_600_000;

/** Vue d'ensemble : ventes, gains, courses, artisanat. */
function Activity() {
  const setPage = useNav((s) => s.setPage);
  const openItem = useNav((s) => s.openItem);
  const ventes = useVentes();
  const [since] = useState(() => Math.floor((Date.now() - WEEK) / 3_600_000) * 3_600_000);
  const journal = useJournal(since);
  const toBuy = useShoppingList((s) => s.items.filter((i) => !i.bought));
  const craft = useQuery({ queryKey: ["craft", "last"], queryFn: api.craftLast });

  const v = ventes.data;
  const out = v?.listings.filter((l) => l.rank !== null && l.rank > v.topRank).length ?? 0;
  const sales = journal.data?.sales ?? [];
  const profit = sales.reduce((t, s) => {
    if (s.costUnits === 0) return t;
    const net = s.unitPrice * s.quantity * (1 - s.taxRate);
    return t + (net * s.costUnits) / s.quantity - s.cost;
  }, 0);
  const crafts = craft.data?.rows.slice(0, 3) ?? [];

  const link = (page: Parameters<typeof setPage>[0], children: ReactNode) => (
    <button
      type="button"
      className="rounded-md border px-3 py-2 text-left hover:bg-muted/40"
      onClick={() => {
        setPage(page);
      }}
    >
      {children}
    </button>
  );

  return (
    <div className="flex flex-col gap-3 px-4 pt-4">
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        {link(
          "ventes",
          <>
            <div className="text-[11px] text-muted-foreground">Mes ventes</div>
            <div className="text-base font-semibold tabular-nums">
              {v?.configured ? `${String(v.listings.length)} annonces` : "—"}
            </div>
            {v?.configured && (out > 0 || v.pending.length > 0) && (
              <div className="text-[11px] text-red-500">
                {[
                  out > 0 ? `${String(out)} hors top ${String(v.topRank)}` : null,
                  v.pending.length > 0 ? `${String(v.pending.length)} à confirmer` : null,
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </div>
            )}
          </>,
        )}
        {link(
          "journal",
          <>
            <div className="text-[11px] text-muted-foreground">Bénéfice réel · 7 jours</div>
            <div
              className={cn(
                "text-base font-semibold tabular-nums",
                profit > 0 && "text-emerald-500",
                profit < 0 && "text-red-500",
              )}
            >
              {sales.length > 0 ? `${profit >= 0 ? "+" : ""}${gilShort(profit)}` : "—"}
            </div>
            <div className="text-[11px] text-muted-foreground">
              {sales.length} vente{sales.length > 1 ? "s" : ""}
            </div>
          </>,
        )}
        {link(
          "courses",
          <>
            <div className="text-[11px] text-muted-foreground">Liste de courses</div>
            <div className="text-base font-semibold tabular-nums">{toBuy.length} à acheter</div>
            {toBuy.length > 0 && (
              <div className="text-[11px] text-muted-foreground">
                {gilShort(toBuy.reduce((t, i) => t + i.cost, 0))} gils
              </div>
            )}
          </>,
        )}
        {link(
          "artisanat",
          <>
            <div className="text-[11px] text-muted-foreground">Artisanat</div>
            {crafts.length === 0 ? (
              <div className="text-base font-semibold">—</div>
            ) : (
              <div className="flex flex-col text-[11px]">
                {crafts.map((c) => (
                  <span key={c.recipeId} className="flex justify-between gap-2">
                    <span
                      className="truncate hover:underline"
                      onClick={(e) => {
                        e.stopPropagation();
                        openItem(c.item.id);
                      }}
                    >
                      {c.item.name}
                    </span>
                    <span className="shrink-0 text-emerald-500 tabular-nums">
                      {gilShort(c.profitPerDay)}/j
                    </span>
                  </span>
                ))}
              </div>
            )}
          </>,
        )}
      </div>
    </div>
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
        <Tile label="Dernier scan">
          <Age ms={r.run.startedAt} staleHours={12} />
        </Tile>
        <Tile label="Opportunités">{r.opportunities.length}</Tile>
        <Tile label="Gils / jour attendus (total)">{gilShort(totalDaily)}</Tile>
        <Tile label="Meilleur trajet / heure">
          {trips[0] ? gilShort(trips[0].profitPerHour) : "—"}
        </Tile>
      </div>

      {trips.length > 0 && (
        <section className="flex flex-col gap-2">
          <h2 className="text-[11px] font-semibold tracking-wider text-muted-foreground uppercase">
            Meilleurs serveurs sources
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
                  <span>{gil(t.profitPerTrip)} / trajet</span>
                  <span>{t.picked.length} objets</span>
                  <span>{t.slotsUsed}/140 emplacements</span>
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
            Meilleures opportunités
          </h2>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              openScan(null);
            }}
          >
            Tous les résultats
          </Button>
        </div>
        {top.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            Le dernier scan n'a rien trouvé au-dessus de tes seuils.
          </p>
        ) : (
          <table className="w-full text-xs">
            <thead className="text-muted-foreground">
              <tr className="border-b">
                <th className="h-7 px-2 text-left font-medium">Objet</th>
                <th className="px-2 text-left font-medium">Source</th>
                <th className="px-2 text-right font-medium">Unités</th>
                <th className="px-2 text-right font-medium">Bénéfice / trajet</th>
                <th className="px-2 text-right font-medium">Gils / jour</th>
                <th className="px-2 text-left font-medium">Confiance</th>
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
