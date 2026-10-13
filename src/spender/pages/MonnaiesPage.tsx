import { useMemo } from "react";
import { DataTable } from "@/components/DataTable";
import { EmptyState } from "@/components/Page";
import { ItemIcon } from "@/components/market";
import { ItemCell, PriceCell, UnlockCell, VendorCell } from "@/spender/cells";
import {
  Filter,
  FilterBar,
  Hint,
  Loadable,
  PageBar,
  RefreshButton,
  WorldSelect,
} from "@/spender/common";
import { spenderApi, type Currency, type PricedOffer } from "@/spender/api";
import { ago, decimal, gil } from "@/lib/format";
import { offersKey, useOffers, useRefresh } from "@/spender/queries";
import { columnHelper } from "@/lib/table";
import { useSpenderWorld } from "@/spender/world";
import { usePrefs } from "@/spender/prefs";

/** En dessous, un objet est considéré comme ne se vendant pas. */
const MIN_SALES_PER_DAY = 1;

function columns(currency: Currency) {
  const c = columnHelper<PricedOffer>();
  return [
    c.accessor("name", {
      header: "Objet",
      cell: (cell) => {
        const o = cell.row.original;
        return <ItemCell name={o.name} icon={o.icon} hq={o.hq} qty={o.quantity} />;
      },
    }),
    c.accessor("cost", {
      header: "Coût",
      cell: (cell) => (
        <span className="inline-flex items-center gap-1">
          {gil(cell.getValue())}
          <ItemIcon path={currency.icon} className="size-4" />
        </span>
      ),
      meta: { numeric: true },
    }),
    c.accessor((o) => o.unitPrice ?? -1, {
      id: "unitPrice",
      header: () => (
        <Hint
          label="Prix HV"
          hint="Prix unitaire retenu : le plus bas entre l'annonce la moins chère et le prix moyen des ventes récentes sur ton serveur."
        />
      ),
      cell: (cell) => {
        const o = cell.row.original;
        return (
          <PriceCell
            value={o.unitPrice}
            minListing={o.minListing}
            averageSale={o.averageSale}
            lastSaleMs={o.lastSaleMs}
          />
        );
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
    c.accessor((o) => o.gilPerCurrency ?? -1, {
      id: "gilPerCurrency",
      header: () => (
        <Hint
          label={`Gils / ${currency.name}`}
          hint="Prix HV × quantité reçue ÷ coût : ce que rapporte chaque unité de monnaie dépensée, avant taxe de vente."
        />
      ),
      cell: (cell) => <span className="font-medium">{gil(cell.row.original.gilPerCurrency)}</span>,
      meta: { numeric: true },
    }),
    c.accessor((o) => o.vendors[0] ?? "", {
      id: "vendor",
      header: "Marchand",
      cell: (cell) => (
        <VendorCell vendors={cell.row.original.vendors} shops={cell.row.original.shops} />
      ),
    }),
    c.accessor((o) => (o.unlock ? 1 : 0), {
      id: "unlock",
      header: "Déblocage",
      cell: (cell) => <UnlockCell unlock={cell.row.original.unlock} />,
    }),
  ];
}

export function MonnaiesPage({ currency }: { currency: Currency }) {
  const prefs = usePrefs();
  const world = useSpenderWorld();
  const offers = useOffers(currency.itemId, world);
  const refresh = useRefresh(offersKey(currency.itemId, world), () =>
    spenderApi.offers(currency.itemId, world, true),
  );
  const cols = useMemo(() => columns(currency), [currency]);

  const rows = useMemo(
    () =>
      (offers.data ?? []).filter(
        (o) =>
          (!prefs.hideLocked || !o.unlock) &&
          (!prefs.hideSlow || (o.dailyVelocity ?? 0) >= MIN_SALES_PER_DAY),
      ),
    [offers.data, prefs.hideLocked, prefs.hideSlow],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageBar>
        <ItemIcon path={currency.icon} className="size-5" />
        <h1 className="text-sm font-semibold">{currency.name}</h1>
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
          id="hide-slow"
          label="Masquer ce qui se vend moins d'une fois par jour"
          checked={prefs.hideSlow}
          onChange={(v) => {
            prefs.set({ hideSlow: v });
          }}
        />
        <Filter
          id="hide-locked"
          label="Masquer les échanges à débloquer"
          checked={prefs.hideLocked}
          onChange={(v) => {
            prefs.set({ hideLocked: v });
          }}
        />
        <span className="ml-auto text-muted-foreground">
          {rows.length} sur {offers.data?.length ?? 0} objets
          {offers.dataUpdatedAt > 0 && ` · prix ${ago(offers.dataUpdatedAt)}`}
        </span>
      </FilterBar>

      <Loadable
        error={offers.error}
        pending={offers.isPending}
        onRetry={() => void offers.refetch()}
      >
        {rows.length === 0 ? (
          <EmptyState
            message={
              (offers.data?.length ?? 0) === 0
                ? "Aucun objet revendable pour cette monnaie."
                : "Aucun objet ne correspond aux filtres."
            }
          />
        ) : (
          <DataTable
            columns={cols}
            data={rows}
            sorting={prefs.sorting.monnaies}
            onSortingChange={(s) => {
              prefs.setSorting("monnaies", s);
            }}
            getRowId={(o) => `${String(o.item)}-${String(o.cost)}-${String(o.quantity)}`}
          />
        )}
      </Loadable>
    </div>
  );
}
