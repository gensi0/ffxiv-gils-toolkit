import { useMemo } from "react";
import { StoreIcon } from "lucide-react";
import { DataTable } from "@/components/DataTable";
import { EmptyState } from "@/components/Page";
import { ItemCell, PriceCell, SignedGil, UnlockCell, VendorCell } from "@/spender/cells";
import {
  Filter,
  FilterBar,
  Hint,
  Loadable,
  PageBar,
  RefreshButton,
  WorldSelect,
} from "@/spender/common";
import { spenderApi, type PricedGilOffer } from "@/spender/api";
import { ago, decimal, gil } from "@/lib/format";
import { gilOffersKey, useGilOffers, useRefresh } from "@/spender/queries";
import { columnHelper } from "@/lib/table";
import { useSpenderWorld } from "@/spender/world";
import { usePrefs } from "@/spender/prefs";

const MIN_SALES_PER_DAY = 1;
/** Au-delà, le tableau devient lent : on n'affiche que les meilleurs. */
const MAX_ROWS = 500;

const c = columnHelper<PricedGilOffer>();
const COLUMNS = [
  c.accessor("name", {
    header: "Objet",
    cell: (cell) => <ItemCell name={cell.getValue()} icon={cell.row.original.icon} />,
  }),
  c.accessor("npcPrice", {
    header: "Prix PNJ",
    cell: (cell) => gil(cell.getValue()),
    meta: { numeric: true },
  }),
  c.accessor((o) => o.unitPrice ?? -1, {
    id: "unitPrice",
    header: () => (
      <Hint
        label="Prix HV"
        hint="Le plus bas entre l'annonce la moins chère et le prix moyen des ventes récentes (qualité normale)."
      />
    ),
    cell: (cell) => {
      const o = cell.row.original;
      return (
        <PriceCell value={o.unitPrice} minListing={o.minListing} averageSale={o.averageSale} />
      );
    },
    meta: { numeric: true },
  }),
  c.accessor((o) => o.profit ?? -Infinity, {
    id: "profit",
    header: () => (
      <Hint
        label="Bénéfice / unité"
        hint="Prix HV moins la taxe de vente de la semaine, moins le prix payé au PNJ."
      />
    ),
    cell: (cell) => <SignedGil value={cell.row.original.profit} />,
    meta: { numeric: true },
  }),
  c.accessor((o) => o.margin ?? -Infinity, {
    id: "margin",
    header: () => <Hint label="Marge" hint="Bénéfice ÷ prix PNJ : ×10 = dix fois la mise." />,
    cell: (cell) => {
      const m = cell.row.original.margin;
      return m == null ? "—" : `×${decimal(m)}`;
    },
    meta: { numeric: true },
  }),
  c.accessor((o) => o.dailyVelocity ?? -1, {
    id: "dailyVelocity",
    header: () => (
      <Hint label="Ventes / jour" hint="Unités vendues par jour sur ton serveur (Universalis)." />
    ),
    cell: (cell) => decimal(cell.row.original.dailyVelocity),
    meta: { numeric: true },
  }),
  c.accessor((o) => o.dailyProfit ?? -Infinity, {
    id: "dailyProfit",
    header: () => (
      <Hint
        label="Bénéfice / jour"
        hint="Bénéfice par unité × ventes par jour : ce que l'objet peut rapporter chaque jour si tu captes toutes les ventes. C'est un plafond, pas une promesse."
      />
    ),
    cell: (cell) => <SignedGil value={cell.row.original.dailyProfit} />,
    meta: { numeric: true },
  }),
  c.accessor((o) => o.vendors[0] ?? "", {
    id: "vendor",
    header: "Marchand",
    cell: (cell) => <VendorCell vendors={cell.row.original.vendors} />,
  }),
  c.accessor((o) => (o.unlock ? 1 : 0), {
    id: "unlock",
    header: "Déblocage",
    cell: (cell) => <UnlockCell unlock={cell.row.original.unlock} />,
  }),
];

export function GilShopPage() {
  const prefs = usePrefs();
  const world = useSpenderWorld();
  const offers = useGilOffers(world);
  const refresh = useRefresh(gilOffersKey(world), () => spenderApi.gilOffers(world, true));

  const filtered = useMemo(
    () =>
      (offers.data ?? []).filter(
        (o) =>
          (!prefs.onlyProfitable || (o.profit ?? 0) > 0) &&
          (!prefs.hideLocked || !o.unlock) &&
          (!prefs.hideSlow || (o.dailyVelocity ?? 0) >= MIN_SALES_PER_DAY),
      ),
    [offers.data, prefs.onlyProfitable, prefs.hideLocked, prefs.hideSlow],
  );
  // Les données arrivent triées par bénéfice / jour : on garde les meilleures.
  const rows = useMemo(() => filtered.slice(0, MAX_ROWS), [filtered]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageBar>
        <StoreIcon className="size-4" />
        <h1 className="text-sm font-semibold">Revente d'objets de marchands PNJ</h1>
        <div className="ml-auto flex items-center gap-2">
          <WorldSelect />
          <RefreshButton
            busy={refresh.isPending || offers.isFetching}
            onClick={() => {
              refresh.mutate();
            }}
          />
        </div>
      </PageBar>

      <FilterBar>
        <Filter
          id="only-profitable"
          label="Seulement les objets rentables"
          checked={prefs.onlyProfitable}
          onChange={(v) => {
            prefs.set({ onlyProfitable: v });
          }}
        />
        <Filter
          id="pnj-hide-slow"
          label="Masquer ce qui se vend moins d'une fois par jour"
          checked={prefs.hideSlow}
          onChange={(v) => {
            prefs.set({ hideSlow: v });
          }}
        />
        <Filter
          id="pnj-hide-locked"
          label="Masquer les objets à débloquer"
          checked={prefs.hideLocked}
          onChange={(v) => {
            prefs.set({ hideLocked: v });
          }}
        />
        <span className="ml-auto text-muted-foreground">
          {filtered.length > MAX_ROWS
            ? `${String(MAX_ROWS)} meilleurs sur ${String(filtered.length)}`
            : `${String(filtered.length)} sur ${String(offers.data?.length ?? 0)} objets`}
          {offers.dataUpdatedAt > 0 && ` · prix ${ago(offers.dataUpdatedAt)}`}
        </span>
      </FilterBar>

      <Loadable
        error={offers.error}
        pending={offers.isPending}
        onRetry={() => void offers.refetch()}
      >
        {rows.length === 0 ? (
          <EmptyState message="Aucun objet ne correspond aux filtres." />
        ) : (
          <DataTable
            columns={COLUMNS}
            data={rows}
            sorting={prefs.sorting.pnj}
            onSortingChange={(s) => {
              prefs.setSorting("pnj", s);
            }}
            getRowId={(o) => String(o.item)}
          />
        )}
      </Loadable>
    </div>
  );
}
