import { useMemo } from "react";
import { PackageCheckIcon } from "lucide-react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { DataTable } from "@/components/DataTable";
import { EmptyState } from "@/components/Page";
import { ItemCell, SignedGil } from "@/spender/cells";
import {
  Filter,
  FilterBar,
  Hint,
  Loadable,
  PageBar,
  RefreshButton,
  WorldSelect,
} from "@/spender/common";
import { spenderApi, GRAND_COMPANIES, type PricedDelivery } from "@/spender/api";
import { ago, decimal, gil } from "@/lib/format";
import { deliveriesKey, useDeliveries, useRefresh } from "@/spender/queries";
import { columnHelper } from "@/lib/table";
import { useSpenderWorld } from "@/spender/world";
import { usePrefs } from "@/spender/prefs";

const MAX_ROWS = 500;

const c = columnHelper<PricedDelivery>();
const COLUMNS = [
  c.accessor("name", {
    header: "Équipement",
    cell: (cell) => <ItemCell name={cell.getValue()} icon={cell.row.original.icon} />,
  }),
  c.accessor("itemLevel", {
    header: "Niv. d'objet",
    meta: { numeric: true },
  }),
  c.accessor("seals", {
    header: "Sceaux",
    cell: (cell) => gil(cell.getValue()),
    meta: { numeric: true },
  }),
  c.accessor((o) => o.buyPrice ?? Infinity, {
    id: "buyPrice",
    header: () => (
      <Hint
        label="Prix HV"
        hint="Annonce la moins chère sur ton serveur, NQ ou HQ (les sceaux sont les mêmes)."
      />
    ),
    cell: (cell) => gil(cell.row.original.buyPrice),
    meta: { numeric: true },
  }),
  c.accessor((o) => o.gilPerSeal ?? Infinity, {
    id: "gilPerSeal",
    header: () => (
      <Hint
        label="Gils / sceau"
        hint="Prix HV ÷ sceaux reçus : ce que te coûte chaque sceau si tu achètes l'objet pour le livrer."
      />
    ),
    cell: (cell) => {
      const v = cell.row.original.gilPerSeal;
      return v == null ? "—" : decimal(v);
    },
    meta: { numeric: true },
  }),
  c.accessor((o) => o.sealsValue ?? -1, {
    id: "sealsValue",
    header: () => (
      <Hint
        label="Valeur des sceaux"
        hint="Sceaux reçus × valeur d'un sceau (voir en haut) : ce que tu peux en tirer en les dépensant."
      />
    ),
    cell: (cell) => gil(cell.row.original.sealsValue),
    meta: { numeric: true },
  }),
  c.accessor((o) => o.profit ?? -Infinity, {
    id: "profit",
    header: () => (
      <Hint
        label="Gain"
        hint="Valeur des sceaux − prix HV. Positif : acheter puis livrer rapporte. Négatif : si tu as déjà l'objet, mieux vaut le vendre."
      />
    ),
    cell: (cell) => <SignedGil value={cell.row.original.profit} />,
    meta: { numeric: true },
  }),
  c.accessor((o) => o.dailyVelocity ?? -1, {
    id: "dailyVelocity",
    header: () => <Hint label="Ventes / jour" hint="Unités vendues par jour, NQ et HQ." />,
    cell: (cell) => decimal(cell.row.original.dailyVelocity),
    meta: { numeric: true },
  }),
];

export function DeliveriesPage() {
  const prefs = usePrefs();
  const world = useSpenderWorld();
  const data = useDeliveries(world, prefs.seal);
  const refresh = useRefresh(deliveriesKey(world, prefs.seal), () =>
    spenderApi.deliveries(world, prefs.seal, true),
  );
  const sealValue = data.data?.sealValue ?? null;

  const filtered = useMemo(
    () =>
      (data.data?.rows ?? []).filter(
        (o) => o.buyPrice !== null && (!prefs.onlyProfitable || (o.profit ?? 0) > 0),
      ),
    [data.data, prefs.onlyProfitable],
  );
  const rows = useMemo(() => filtered.slice(0, MAX_ROWS), [filtered]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageBar>
        <PackageCheckIcon className="size-4" />
        <h1 className="text-sm font-semibold">Livraisons expertes de grande compagnie</h1>
        <div className="ml-auto flex items-center gap-2">
          <Select
            value={String(prefs.seal)}
            onValueChange={(v) => {
              prefs.set({ seal: Number(v) });
            }}
          >
            <SelectTrigger size="sm" className="w-36">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {GRAND_COMPANIES.map((g) => (
                <SelectItem key={g.seal} value={String(g.seal)}>
                  {g.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <WorldSelect />
          <RefreshButton
            busy={refresh.isPending || data.isFetching}
            onClick={() => {
              refresh.mutate();
            }}
          />
        </div>
      </PageBar>

      <FilterBar>
        <span>
          Valeur d'un sceau :{" "}
          {sealValue ? (
            <Hint
              label={`${decimal(sealValue.gil)} gils`}
              hint={`Meilleur échange de sceaux qui s'écoule en volume sur ton serveur : ${sealValue.itemName} (${gil(sealValue.cost)} sceaux), taxe de vente de la semaine déduite. Suppose que tu as le grade requis.`}
            />
          ) : (
            "—"
          )}
        </span>
        <Filter
          id="del-only-profitable"
          label="Seulement les livraisons rentables"
          checked={prefs.onlyProfitable}
          onChange={(v) => {
            prefs.set({ onlyProfitable: v });
          }}
        />
        <span className="ml-auto text-muted-foreground">
          {filtered.length > MAX_ROWS
            ? `${String(MAX_ROWS)} meilleurs sur ${String(filtered.length)}`
            : `${String(filtered.length)} équipements`}
          {data.dataUpdatedAt > 0 && ` · prix ${ago(data.dataUpdatedAt)}`}
        </span>
      </FilterBar>
      <p className="border-b px-4 py-1.5 text-[11px] text-muted-foreground">
        Rappel : les sceaux sont plafonnés selon ton grade, au-delà les livraisons ne rapportent
        plus rien.
      </p>

      <Loadable error={data.error} pending={data.isPending} onRetry={() => void data.refetch()}>
        {rows.length === 0 ? (
          <EmptyState message="Aucun équipement ne correspond aux filtres." />
        ) : (
          <DataTable
            columns={COLUMNS}
            data={rows}
            sorting={prefs.sorting.livraisons}
            onSortingChange={(s) => {
              prefs.setSorting("livraisons", s);
            }}
            getRowId={(o) => String(o.item)}
          />
        )}
      </Loadable>
    </div>
  );
}
