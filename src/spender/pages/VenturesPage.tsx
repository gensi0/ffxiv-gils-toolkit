import { useMemo } from "react";
import { UsersIcon } from "lucide-react";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { DataTable } from "@/components/DataTable";
import { EmptyState } from "@/components/Page";
import { ItemCell, PriceCell } from "@/spender/cells";
import {
  Filter,
  FilterBar,
  Hint,
  Loadable,
  PageBar,
  RefreshButton,
  WorldSelect,
} from "@/spender/common";
import { spenderApi, VENTURE_CLASSES, type PricedVenture, type VentureClass } from "@/spender/api";
import { ago, decimal, gil } from "@/lib/format";
import { useRefresh, useVentures, venturesKey } from "@/spender/queries";
import { columnHelper } from "@/lib/table";
import { useSpenderWorld } from "@/spender/world";
import { usePrefs, type Retainer } from "@/spender/prefs";

const TAX = 0.05;
const MIN_SALES_PER_DAY = 1;

/** Mission chiffrée pour le servant renseigné. */
interface Row extends PricedVenture {
  available: boolean;
  /** Palier atteint, 1 à 5. */
  tier: number;
  quantity: number;
  gilPerVenture: number | null;
}

function evaluate(v: PricedVenture, r: Retainer): Row {
  const tier = 1 + v.thresholds.filter((t) => t > 0 && r.stat >= t).length;
  const quantity = v.quantities[tier - 1] ?? v.quantities[0] ?? 0;
  return {
    ...v,
    available: r.level >= v.retainerLevel && r.stat >= v.requiredStat,
    tier,
    quantity,
    gilPerVenture: v.unitPrice == null ? null : v.unitPrice * quantity * (1 - TAX),
  };
}

function columns(statLabel: string) {
  const c = columnHelper<Row>();
  return [
    c.accessor("name", {
      header: "Objet rapporté",
      cell: (cell) => <ItemCell name={cell.getValue()} icon={cell.row.original.icon} />,
    }),
    c.accessor("retainerLevel", { header: "Niv. requis", meta: { numeric: true } }),
    c.accessor("tier", {
      header: () => (
        <Hint
          label="Palier"
          hint={`Palier atteint avec ta valeur de ${statLabel.toLowerCase()} : plus il est haut, plus le servant rapporte.`}
        />
      ),
      cell: (cell) => {
        const v = cell.row.original;
        return (
          <Tooltip>
            <TooltipTrigger asChild>
              <span className="cursor-help">{cell.getValue()} / 5</span>
            </TooltipTrigger>
            <TooltipContent>
              <div className="flex flex-col gap-0.5">
                {v.quantities.map((q, i) => (
                  <span key={i}>
                    Palier {i + 1} : ×{q}
                    {i > 0 &&
                      ` (${statLabel.toLowerCase()} ≥ ${String(v.thresholds[i - 1] ?? "?")})`}
                  </span>
                ))}
              </div>
            </TooltipContent>
          </Tooltip>
        );
      },
      meta: { numeric: true },
    }),
    c.accessor("quantity", {
      header: "Quantité",
      cell: (cell) => `×${String(cell.getValue())}`,
      meta: { numeric: true },
    }),
    c.accessor((o) => o.unitPrice ?? -1, {
      id: "unitPrice",
      header: "Prix HV",
      cell: (cell) => {
        const o = cell.row.original;
        return (
          <PriceCell value={o.unitPrice} minListing={o.minListing} averageSale={o.averageSale} />
        );
      },
      meta: { numeric: true },
    }),
    c.accessor((o) => o.gilPerVenture ?? -1, {
      id: "gilPerVenture",
      header: () => (
        <Hint
          label="Gils / mission"
          hint="Prix HV × quantité, taxe de 5 % déduite. Une mission dure 1 h et coûte 1 jeton de mission."
        />
      ),
      cell: (cell) => <span className="font-medium">{gil(cell.row.original.gilPerVenture)}</span>,
      meta: { numeric: true },
    }),
    c.accessor((o) => o.dailyVelocity ?? -1, {
      id: "dailyVelocity",
      header: () => (
        <Hint
          label="Ventes / jour"
          hint="Unités vendues par jour. À comparer à la quantité : 24 missions par jour inondent vite un petit marché."
        />
      ),
      cell: (cell) => decimal(cell.row.original.dailyVelocity),
      meta: { numeric: true },
    }),
  ];
}

export function VenturesPage() {
  const prefs = usePrefs();
  const world = useSpenderWorld();
  const data = useVentures(world);
  const refresh = useRefresh(venturesKey(world), () => spenderApi.ventures(world, true));
  const statLabel =
    VENTURE_CLASSES.find((c) => c.id === prefs.ventureClass)?.stat ?? "Niveau d'objet moyen";
  const retainer = prefs.retainers[prefs.ventureClass];
  const cols = useMemo(() => columns(statLabel), [statLabel]);

  const rows = useMemo(
    () =>
      (data.data ?? [])
        .filter((v) => v.class === prefs.ventureClass)
        .map((v) => evaluate(v, retainer))
        .filter(
          (v) => v.available && (!prefs.hideSlow || (v.dailyVelocity ?? 0) >= MIN_SALES_PER_DAY),
        ),
    [data.data, prefs.ventureClass, retainer, prefs.hideSlow],
  );

  const setNumber = (key: keyof Retainer) => (e: React.ChangeEvent<HTMLInputElement>) => {
    const n = Number.parseInt(e.target.value, 10);
    prefs.setRetainer(prefs.ventureClass, { [key]: Number.isNaN(n) ? 0 : n });
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageBar>
        <UsersIcon className="size-4" />
        <h1 className="text-sm font-semibold">Missions de servants</h1>
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
        <ToggleGroup
          type="single"
          size="sm"
          variant="outline"
          value={prefs.ventureClass}
          onValueChange={(v) => {
            if (v) prefs.set({ ventureClass: v as VentureClass });
          }}
        >
          {VENTURE_CLASSES.map((c) => (
            <ToggleGroupItem key={c.id} value={c.id} className="px-3 text-xs">
              {c.label}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        <div className="flex items-center gap-1.5">
          <Label htmlFor="ret-level">Niveau du servant</Label>
          <Input
            id="ret-level"
            type="number"
            min={1}
            max={100}
            className="h-7 w-16 text-xs"
            value={retainer.level}
            onChange={setNumber("level")}
          />
        </div>
        <div className="flex items-center gap-1.5">
          <Label htmlFor="ret-stat">{statLabel}</Label>
          <Input
            id="ret-stat"
            type="number"
            min={0}
            className="h-7 w-20 text-xs"
            value={retainer.stat}
            onChange={setNumber("stat")}
          />
        </div>
        <Filter
          id="ven-hide-slow"
          label="Masquer ce qui se vend moins d'une fois par jour"
          checked={prefs.hideSlow}
          onChange={(v) => {
            prefs.set({ hideSlow: v });
          }}
        />
        <span className="ml-auto text-muted-foreground">
          {rows.length} missions accessibles
          {data.dataUpdatedAt > 0 && ` · prix ${ago(data.dataUpdatedAt)}`}
        </span>
      </FilterBar>

      <Loadable error={data.error} pending={data.isPending} onRetry={() => void data.refetch()}>
        {rows.length === 0 ? (
          <EmptyState message="Aucune mission accessible avec ce servant." />
        ) : (
          <DataTable
            columns={cols}
            data={rows}
            sorting={prefs.sorting.servants}
            onSortingChange={(s) => {
              prefs.setSorting("servants", s);
            }}
            getRowId={(o) => `${o.class}-${String(o.item)}`}
          />
        )}
      </Loadable>
    </div>
  );
}
