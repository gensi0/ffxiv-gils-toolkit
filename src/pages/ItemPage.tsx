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
    <PageHeader title={detail.data?.item.name ?? "Objet"}>
      <Button variant="ghost" size="sm" onClick={onBack}>
        <ArrowLeftIcon />
        Retour
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
        Actualiser
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
            Réessayer
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
            {item.levelItem != null && <span>Niv. d'objet {item.levelItem}</span>}
            <span>Pile de {item.stackSize}</span>
            <span>{item.canBeHq ? "HQ possible" : "NQ uniquement"}</span>
            <span>
              Serveur principal {d.homeWorld} · taxe de vente {percent(d.sellerTax)}
            </span>
            <span>
              Récupéré <Age ms={d.fetchedAt} />
            </span>
          </div>
        </div>
      </div>
      {d.failedDcs.length > 0 && (
        <p className="text-xs text-destructive">
          Annonces indisponibles pour : {d.failedDcs.join(", ")}.
        </p>
      )}

      <Section title="Arbitrage">
        {d.arbitrage.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            Aucun serveur source ne bat l'HV de ton serveur principal après taxes, dans ton horizon
            de vente.
          </p>
        ) : (
          <ArbitrageTable rows={d.arbitrage} />
        )}
      </Section>

      <Section title={`Historique des ventes · ${d.homeWorld} · 28 jours`}>
        <SalesChart sales={d.sales} canBeHq={item.canBeHq} now={d.fetchedAt} />
      </Section>

      <Section title={`HV du serveur principal · ${d.homeWorld}`}>
        <div className="grid gap-4 md:grid-cols-2">
          <HomeStatsCard label="NQ" s={d.homeNq} />
          {item.canBeHq && <HomeStatsCard label="HQ" s={d.homeHq} />}
        </div>
      </Section>

      <Section title="Prix par serveur">
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
        label="Unités vendues / jour (7 j · 28 j)"
        value={`${decimal(s.d7.unitsPerDay)} · ${decimal(s.d28.unitsPerDay)}`}
      />
      <Stat label="Ventes / jour (7 j)" value={decimal(s.d7.salesPerDay)} />
      <Stat label="Moyenne (7 j · 28 j)" value={`${gil(s.d7.average)} · ${gil(s.d28.average)}`} />
      <Stat label="Médiane (7 j · 28 j)" value={`${gil(s.d7.median)} · ${gil(s.d28.median)}`} />
      <Stat label="Annonce crédible la moins chère" value={gil(s.f5?.floorListing)} />
      <Stat label="Prix de vente attendu" value={gil(s.f5?.expectedSell)} />
      <Stat
        label="Jours de stock en vente"
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
    header: "Achat à",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  arb.accessor("unitsToBuy", { header: "Unités", meta: { numeric: true } }),
  arb.accessor("expectedSell", {
    header: "Vente à",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  arb.accessor("unitProfit", {
    header: "Bénéfice / unité",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  arb.accessor("profitPerTrip", {
    header: "Bénéfice / trajet",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  arb.accessor("dailyProfit", {
    header: "Gils / jour",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  arb.accessor("confidence", {
    header: "Confiance",
    cell: (c) => <ConfidenceBadge value={c.getValue()} />,
  }),
  arb.accessor("sourceUploadMs", {
    header: "Âge des données",
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
        header: "Serveur",
        cell: (c) => (
          <span className={cn(c.row.original.isHome && "font-semibold")}>
            {c.getValue()}
            {c.row.original.isHome && (
              <span className="ml-1 text-[11px] text-muted-foreground">principal</span>
            )}
          </span>
        ),
      }),
      wc.accessor("dc", { header: "DC" }),
      wc.accessor((r) => r.nq.minPrice ?? undefined, {
        id: "nqMin",
        header: "Min NQ",
        cell: (c) => gil(c.getValue()),
        meta: { numeric: true },
        sortUndefined: "last",
      }),
      wc.accessor((r) => r.nq.unitsAtMin, {
        id: "nqUnits",
        header: "Unités au min NQ",
        meta: { numeric: true },
      }),
      wc.accessor((r) => r.hq.minPrice ?? undefined, {
        id: "hqMin",
        header: "Min HQ",
        cell: (c) => gil(c.getValue()),
        meta: { numeric: true },
        sortUndefined: "last",
      }),
      wc.accessor((r) => r.hq.unitsAtMin, {
        id: "hqUnits",
        header: "Unités au min HQ",
        meta: { numeric: true },
      }),
      wc.accessor((r) => r.nq.listings + r.hq.listings, {
        id: "listings",
        header: "Annonces",
        meta: { numeric: true },
      }),
      wc.accessor("uploadMs", {
        header: "Âge des données",
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
