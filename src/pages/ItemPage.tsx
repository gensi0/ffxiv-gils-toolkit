import { useMemo, type ReactNode } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ArrowLeftIcon, RefreshCwIcon } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { PageHeader } from "@/components/Page";
import { WatchButton } from "@/components/WatchButton";
import { SalesChart } from "@/components/SalesChart";
import { DataTable } from "@/components/DataTable";
import { columnHelper } from "@/lib/table";
import { Age, ConfidenceBadge, ItemIcon, QualityTag } from "@/components/market";
import {
  api,
  errorMessage,
  type ArbitrageRow,
  type ItemDetail,
  type QualityHomeStats,
  type WorldRow,
} from "@/lib/api";
import { decimal, gil, percent } from "@/lib/format";
import { useItemDetail } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { useNav } from "@/store/nav";

export function ItemPage() {
  const itemId = useNav((s) => s.itemId);
  const goBack = useNav((s) => s.goBack);
  if (itemId === null) return null;
  return <ItemView itemId={itemId} onBack={goBack} />;
}

function ItemView({ itemId, onBack }: { itemId: number; onBack: () => void }) {
  const detail = useItemDetail(itemId);
  const qc = useQueryClient();
  const refresh = useMutation({
    mutationFn: () => api.itemDetail(itemId, true),
    onSuccess: (d) => {
      qc.setQueryData(["item", itemId], d);
    },
    onError: (e) => toast.error(errorMessage(e)),
  });

  const header = (
    <PageHeader title={detail.data?.item.name ?? "Item"}>
      <Button variant="ghost" size="sm" onClick={onBack}>
        <ArrowLeftIcon />
        Back
      </Button>
      <WatchButton itemId={itemId} />
      <Button
        variant="outline"
        size="sm"
        disabled={refresh.isPending || detail.isPending}
        onClick={() => {
          refresh.mutate();
        }}
      >
        <RefreshCwIcon className={cn(refresh.isPending && "animate-spin")} />
        Refresh
      </Button>
    </PageHeader>
  );

  if (detail.isError) {
    return (
      <>
        {header}
        <div className="flex items-center gap-3 p-4 text-xs text-destructive">
          {errorMessage(detail.error)}
          <Button variant="outline" size="sm" onClick={() => void detail.refetch()}>
            Retry
          </Button>
        </div>
      </>
    );
  }
  if (!detail.data) {
    return (
      <>
        {header}
        <div className="flex flex-col gap-3 p-4">
          <Skeleton className="h-10 w-80" />
          <Skeleton className="h-32" />
          <Skeleton className="h-64" />
        </div>
      </>
    );
  }
  return (
    <>
      {header}
      <DetailBody d={detail.data} />
    </>
  );
}

function DetailBody({ d }: { d: ItemDetail }) {
  const { item } = d;
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto p-4">
      <div className="flex items-center gap-3">
        <ItemIcon path={item.iconPath} className="size-10" />
        <div className="flex flex-col gap-0.5">
          <div className="text-sm font-semibold">{item.name}</div>
          <div className="flex gap-3 text-[11px] text-muted-foreground">
            {item.category && <span>{item.category}</span>}
            {item.levelItem != null && <span>iLvl {item.levelItem}</span>}
            <span>Stack {item.stackSize}</span>
            <span>{item.canBeHq ? "HQ-able" : "NQ only"}</span>
            <span>
              Home {d.homeWorld} · seller tax {percent(d.sellerTax)}
            </span>
            <span>
              Fetched <Age ms={d.fetchedAt} />
            </span>
          </div>
        </div>
      </div>
      {d.failedDcs.length > 0 && (
        <p className="text-xs text-destructive">
          Listings unavailable for: {d.failedDcs.join(", ")}.
        </p>
      )}

      <Section title="Arbitrage">
        {d.arbitrage.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            No source world beats the home market after taxes, within your sell horizon.
          </p>
        ) : (
          <ArbitrageTable rows={d.arbitrage} />
        )}
      </Section>

      <Section title={`Sales history · ${d.homeWorld} · 28 days`}>
        <SalesChart sales={d.sales} canBeHq={item.canBeHq} now={d.fetchedAt} />
      </Section>

      <Section title={`Home market · ${d.homeWorld}`}>
        <div className="grid gap-4 md:grid-cols-2">
          <HomeStatsCard label="NQ" s={d.homeNq} />
          {item.canBeHq && <HomeStatsCard label="HQ" s={d.homeHq} />}
        </div>
      </Section>

      <Section title="Prices by world">
        <WorldTable rows={d.worlds} />
      </Section>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2">
      <h2 className="text-[11px] font-semibold tracking-wider text-muted-foreground uppercase">
        {title}
      </h2>
      {children}
    </section>
  );
}

function Stat({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex justify-between gap-3 border-b border-border/50 py-1 text-xs">
      <span className="text-muted-foreground">{label}</span>
      <span className="tabular-nums">{value}</span>
    </div>
  );
}

function HomeStatsCard({ label, s }: { label: string; s: QualityHomeStats }) {
  return (
    <div className="rounded-md border p-3">
      <div className="mb-1 text-xs font-medium">{label}</div>
      <Stat
        label="Units sold / day (7d · 28d)"
        value={`${decimal(s.d7.unitsPerDay)} · ${decimal(s.d28.unitsPerDay)}`}
      />
      <Stat label="Sales / day (7d)" value={decimal(s.d7.salesPerDay)} />
      <Stat label="Average (7d · 28d)" value={`${gil(s.d7.average)} · ${gil(s.d28.average)}`} />
      <Stat label="Median (7d · 28d)" value={`${gil(s.d7.median)} · ${gil(s.d28.median)}`} />
      <Stat label="Cheapest credible listing" value={gil(s.f5?.floorListing)} />
      <Stat label="Expected sell price" value={gil(s.f5?.expectedSell)} />
      <Stat
        label="Days of supply listed"
        value={s.f5?.daysOfSupply != null ? decimal(s.f5.daysOfSupply) : "—"}
      />
    </div>
  );
}

const arb = columnHelper<ArbitrageRow>();
const arbColumns = [
  arb.accessor("world", {
    header: "Source",
    cell: (c) => (
      <span>
        {c.getValue()} <span className="text-muted-foreground">· {c.row.original.dc}</span>
      </span>
    ),
  }),
  arb.accessor("quality", {
    header: "Q",
    cell: (c) => <QualityTag quality={c.getValue()} soldAs={c.row.original.soldAs} />,
  }),
  arb.accessor("sourceUnitPrice", {
    header: "Buy at",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  arb.accessor("unitsToBuy", { header: "Units", meta: { numeric: true } }),
  arb.accessor("expectedSell", {
    header: "Sell at",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  arb.accessor("unitProfit", {
    header: "Profit / unit",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  arb.accessor("profitPerTrip", {
    header: "Profit / trip",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  arb.accessor("dailyProfit", {
    header: "Gil / day",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  arb.accessor("confidence", {
    header: "Confidence",
    cell: (c) => <ConfidenceBadge value={c.getValue()} />,
  }),
  arb.accessor("sourceUploadMs", {
    header: "Data age",
    cell: (c) => <Age ms={c.getValue()} />,
    meta: { numeric: true },
  }),
];

function ArbitrageTable({ rows }: { rows: ArbitrageRow[] }) {
  return (
    <div className="flex max-h-80 flex-col rounded-md border">
      <DataTable
        columns={arbColumns}
        data={rows}
        initialSorting={[{ id: "dailyProfit", desc: true }]}
        getRowId={(r) => `${r.worldId}-${r.quality}`}
      />
    </div>
  );
}

const wc = columnHelper<WorldRow>();

function WorldTable({ rows }: { rows: WorldRow[] }) {
  const columns = useMemo(
    () => [
      wc.accessor("world", {
        header: "World",
        cell: (c) => (
          <span className={cn(c.row.original.isHome && "font-semibold")}>
            {c.getValue()}
            {c.row.original.isHome && (
              <span className="ml-1 text-[11px] text-muted-foreground">home</span>
            )}
          </span>
        ),
      }),
      wc.accessor("dc", { header: "DC" }),
      wc.accessor((r) => r.nq.minPrice ?? undefined, {
        id: "nqMin",
        header: "NQ min",
        cell: (c) => gil(c.getValue()),
        meta: { numeric: true },
        sortUndefined: "last",
      }),
      wc.accessor((r) => r.nq.unitsAtMin, {
        id: "nqUnits",
        header: "Units @ NQ min",
        meta: { numeric: true },
      }),
      wc.accessor((r) => r.hq.minPrice ?? undefined, {
        id: "hqMin",
        header: "HQ min",
        cell: (c) => gil(c.getValue()),
        meta: { numeric: true },
        sortUndefined: "last",
      }),
      wc.accessor((r) => r.hq.unitsAtMin, {
        id: "hqUnits",
        header: "Units @ HQ min",
        meta: { numeric: true },
      }),
      wc.accessor((r) => r.nq.listings + r.hq.listings, {
        id: "listings",
        header: "Listings",
        meta: { numeric: true },
      }),
      wc.accessor("uploadMs", {
        header: "Data age",
        cell: (c) => <Age ms={c.getValue()} />,
        meta: { numeric: true },
      }),
    ],
    [],
  );
  return (
    <div className="flex max-h-[28rem] flex-col rounded-md border">
      <DataTable
        columns={columns}
        data={rows}
        initialSorting={[{ id: "nqMin", desc: false }]}
        getRowId={(r) => String(r.worldId)}
        rowClassName={(r) => (r.isHome ? "bg-muted/40" : undefined)}
      />
    </div>
  );
}
