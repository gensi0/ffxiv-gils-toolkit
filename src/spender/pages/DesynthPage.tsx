import { useMemo } from "react";
import { HammerIcon, InfoIcon } from "lucide-react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { DataTable } from "@/components/DataTable";
import { EmptyState } from "@/components/Page";
import { ItemIcon } from "@/components/market";
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
import { spenderApi, type PricedDesynth } from "@/spender/api";
import { ago, decimal, gil } from "@/lib/format";
import { desynthsKey, useDesynths, useRefresh } from "@/spender/queries";
import { columnHelper } from "@/lib/table";
import { useSpenderWorld } from "@/spender/world";
import { usePrefs } from "@/spender/prefs";

const MIN_SALES_PER_DAY = 1;
const MAX_ROWS = 500;

const c = columnHelper<PricedDesynth>();
const COLUMNS = [
  c.accessor("name", {
    header: "Objet",
    cell: (cell) => <ItemCell name={cell.getValue()} icon={cell.row.original.icon} />,
  }),
  c.accessor((o) => o.buyPrice ?? Infinity, {
    id: "buyPrice",
    header: () => <Hint label="Prix HV" hint="Annonce la moins chère, NQ ou HQ." />,
    cell: (cell) => gil(cell.row.original.buyPrice),
    meta: { numeric: true },
  }),
  c.accessor((o) => o.results.length, {
    id: "results",
    header: "Résultats possibles",
    cell: (cell) => {
      const r = cell.row.original.results;
      return (
        <Tooltip>
          <TooltipTrigger asChild>
            <span className="inline-flex cursor-help items-center gap-1">
              {r.slice(0, 4).map((x) => (
                <ItemIcon key={x.item} path={x.icon} className="size-4" />
              ))}
              {r.length > 4 && <span className="text-muted-foreground">+{r.length - 4}</span>}
            </span>
          </TooltipTrigger>
          <TooltipContent className="max-w-80">
            <div className="flex flex-col gap-0.5">
              {r.map((x) => (
                <span key={x.item}>
                  {x.name} : {gil(x.unitPrice)} ({decimal(x.dailyVelocity)} ventes / jour)
                </span>
              ))}
            </div>
          </TooltipContent>
        </Tooltip>
      );
    },
    meta: { numeric: true },
  }),
  c.accessor((o) => o.bestResult ?? -1, {
    id: "bestResult",
    header: () => (
      <Hint
        label="Meilleur résultat"
        hint="Prix du résultat possible le plus cher qui se vend au moins une fois par jour, taxe déduite. C'est le meilleur cas : la plupart des désynthèses donnent un résultat moins cher."
      />
    ),
    cell: (cell) => gil(cell.row.original.bestResult),
    meta: { numeric: true },
  }),
  c.accessor((o) => o.bestGain ?? -Infinity, {
    id: "bestGain",
    header: () => (
      <Hint
        label="Gain max"
        hint="Meilleur résultat − prix d'achat. Un plafond, pas une moyenne."
      />
    ),
    cell: (cell) => <SignedGil value={cell.row.original.bestGain} />,
    meta: { numeric: true },
  }),
  c.accessor((o) => o.dailyVelocity ?? -1, {
    id: "dailyVelocity",
    header: () => <Hint label="Ventes / jour" hint="Ventes par jour de l'objet à désynthétiser." />,
    cell: (cell) => decimal(cell.row.original.dailyVelocity),
    meta: { numeric: true },
  }),
];

export function DesynthPage() {
  const prefs = usePrefs();
  const world = useSpenderWorld();
  const data = useDesynths(world);
  const refresh = useRefresh(desynthsKey(world), () => spenderApi.desynths(world, true));

  const filtered = useMemo(
    () =>
      (data.data ?? []).filter(
        (o) =>
          o.buyPrice !== null &&
          (!prefs.onlyProfitable || (o.bestGain ?? 0) > 0) &&
          (!prefs.hideSlow || (o.dailyVelocity ?? 0) >= MIN_SALES_PER_DAY),
      ),
    [data.data, prefs.onlyProfitable, prefs.hideSlow],
  );
  const rows = useMemo(() => filtered.slice(0, MAX_ROWS), [filtered]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageBar>
        <HammerIcon className="size-4" />
        <h1 className="text-sm font-semibold">Désynthèse</h1>
        <div className="ml-auto flex items-center gap-2">
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
        <Filter
          id="des-only-profitable"
          label="Seulement si le meilleur résultat est rentable"
          checked={prefs.onlyProfitable}
          onChange={(v) => {
            prefs.set({ onlyProfitable: v });
          }}
        />
        <Filter
          id="des-hide-slow"
          label="Masquer ce qui se vend moins d'une fois par jour"
          checked={prefs.hideSlow}
          onChange={(v) => {
            prefs.set({ hideSlow: v });
          }}
        />
        <span className="ml-auto text-muted-foreground">
          {filtered.length > MAX_ROWS
            ? `${String(MAX_ROWS)} meilleurs sur ${String(filtered.length)}`
            : `${String(filtered.length)} objets`}
          {data.dataUpdatedAt > 0 && ` · prix ${ago(data.dataUpdatedAt)}`}
        </span>
      </FilterBar>
      <p className="flex items-center gap-1.5 border-b px-4 py-1.5 text-[11px] text-muted-foreground">
        <InfoIcon className="size-3 shrink-0" />
        Les taux de désynthèse ne sont pas publics : l'app connaît les résultats possibles (données
        communautaires Teamcraft), pas leur probabilité. Le gain affiché est le meilleur cas.
      </p>

      <Loadable error={data.error} pending={data.isPending} onRetry={() => void data.refetch()}>
        {rows.length === 0 ? (
          <EmptyState message="Aucun objet ne correspond aux filtres." />
        ) : (
          <DataTable
            columns={COLUMNS}
            data={rows}
            sorting={prefs.sorting.desynthese}
            onSortingChange={(s) => {
              prefs.setSorting("desynthese", s);
            }}
            getRowId={(o) => String(o.item)}
          />
        )}
      </Loadable>
    </div>
  );
}
