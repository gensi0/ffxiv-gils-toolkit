import { useMemo } from "react";
import { ShoppingCartIcon, Trash2Icon, XIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { EmptyState, PageHeader } from "@/components/Page";
import { LotChips, LotsTooltip } from "@/components/Lots";
import { ItemIcon, QualityTag } from "@/components/market";
import { ago, gil, percent } from "@/lib/format";
import { averagePrice, margin } from "@/lib/purchase";
import { cn } from "@/lib/utils";
import { useNav } from "@/store/nav";
import { type ShoppingItem, useShoppingList } from "@/store/shoppingList";

interface WorldGroup {
  world: string;
  dc: string;
  crossRegion: boolean;
  items: ShoppingItem[];
  cost: number;
  profit: number;
}

interface DcGroup {
  dc: string;
  worlds: WorldGroup[];
  cost: number;
  profit: number;
}

const sum = (items: ShoppingItem[], f: (i: ShoppingItem) => number) =>
  items.reduce((t, i) => t + f(i), 0);

/** Regroupe par centre de données puis par serveur, les plus rentables d'abord. */
function group(items: ShoppingItem[]): DcGroup[] {
  const byDc = new Map<string, Map<string, ShoppingItem[]>>();
  for (const i of items) {
    const worlds = byDc.get(i.dc) ?? new Map<string, ShoppingItem[]>();
    worlds.set(i.world, [...(worlds.get(i.world) ?? []), i]);
    byDc.set(i.dc, worlds);
  }
  return [...byDc.entries()]
    .map(([dc, worlds]) => {
      const ws: WorldGroup[] = [...worlds.entries()]
        .map(([world, list]) => ({
          world,
          dc,
          crossRegion: list[0]?.crossRegion ?? false,
          items: [...list].sort((a, b) => b.profit - a.profit),
          cost: sum(list, (i) => i.cost),
          profit: sum(list, (i) => i.profit),
        }))
        .sort((a, b) => b.profit - a.profit);
      return {
        dc,
        worlds: ws,
        cost: ws.reduce((t, w) => t + w.cost, 0),
        profit: ws.reduce((t, w) => t + w.profit, 0),
      };
    })
    .sort((a, b) => b.profit - a.profit);
}

function Total({ label, value, accent }: { label: string; value: string; accent?: boolean }) {
  return (
    <div className="flex flex-col rounded-md border px-3 py-2">
      <span className="text-[11px] text-muted-foreground">{label}</span>
      <span className={cn("text-base font-semibold tabular-nums", accent && "text-emerald-500")}>
        {value}
      </span>
    </div>
  );
}

function WorldCard({ g }: { g: WorldGroup }) {
  const { remove, setBought } = useShoppingList();
  const openItem = useNav((s) => s.openItem);
  const done = g.items.every((i) => i.bought);
  return (
    <section className={cn("flex flex-col rounded-md border text-xs", done && "opacity-60")}>
      <header className="flex items-baseline gap-2 border-b px-3 py-2">
        <span className="text-sm font-semibold">{g.world}</span>
        {g.crossRegion && (
          <span className="text-[11px] text-amber-600 dark:text-amber-400">inter-région</span>
        )}
        <span className="ml-auto text-muted-foreground tabular-nums">{gil(g.cost)} à dépenser</span>
        <span className="font-semibold text-emerald-500 tabular-nums">+{gil(g.profit)}</span>
        <span className="text-[11px] text-muted-foreground tabular-nums">
          marge {percent(margin(g.profit, g.cost))}
        </span>
      </header>
      <table className="w-full">
        <tbody>
          {g.items.map((i) => (
            <tr key={i.key} className="border-t border-border/50 first:border-t-0">
              <td className="w-8 pl-3">
                <Checkbox
                  checked={i.bought}
                  aria-label="Acheté"
                  onCheckedChange={(c) => {
                    setBought(i.key, c === true);
                  }}
                />
              </td>
              <td className="h-8 px-2">
                <button
                  type="button"
                  className={cn(
                    "flex items-center gap-2 text-left hover:underline",
                    i.bought && "text-muted-foreground line-through",
                  )}
                  onClick={() => {
                    openItem(i.itemId);
                  }}
                >
                  <ItemIcon path={i.iconPath} className="size-4" />
                  <span className="max-w-52 truncate">{i.name}</span>
                  <QualityTag quality={i.quality} soldAs={i.soldAs} />
                </button>
                {i.lots && i.lots.length > 1 && !i.bought && (
                  <span className="mt-0.5 block pl-6">
                    <LotChips lots={i.lots} />
                  </span>
                )}
              </td>
              <td className="px-2 text-right whitespace-nowrap tabular-nums">
                <LotsTooltip lots={i.lots ?? []} max={i.maxUnitPrice ?? null}>
                  <span className="font-medium">{gil(i.units)}</span>
                  <span className="text-muted-foreground">
                    {" "}
                    × moy. {gil(averagePrice(i.lots ?? [], i.units, i.cost))}
                  </span>
                </LotsTooltip>
                {i.maxUnitPrice != null && (
                  <span className="block text-[10px] text-muted-foreground">
                    max {gil(i.maxUnitPrice)} / u
                  </span>
                )}
              </td>
              <td className="px-2 text-right text-muted-foreground tabular-nums">
                revente {gil(i.sellPrice)}
              </td>
              <td className="px-2 text-right text-emerald-500 tabular-nums">+{gil(i.profit)}</td>
              <td className="w-8 pr-2">
                <button
                  type="button"
                  aria-label="Retirer"
                  className="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
                  onClick={() => {
                    remove(i.key);
                  }}
                >
                  <XIcon className="size-3.5" />
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

export function ShoppingListPage() {
  const { items, clearBought, clear } = useShoppingList();
  const groups = useMemo(() => group(items), [items]);
  const left = items.filter((i) => !i.bought);
  const oldest = items.length > 0 ? Math.min(...items.map((i) => i.addedAt)) : null;
  const worlds = new Set(left.map((i) => i.world)).size;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader title="Liste de courses">
        <Button
          variant="outline"
          size="sm"
          disabled={!items.some((i) => i.bought)}
          onClick={clearBought}
        >
          Retirer les achetés
        </Button>
        <Button
          variant="outline"
          size="sm"
          disabled={items.length === 0}
          onClick={() => {
            if (window.confirm("Vider toute la liste de courses ?")) clear();
          }}
        >
          <Trash2Icon />
          Vider
        </Button>
      </PageHeader>

      {items.length === 0 ? (
        <EmptyState message="Ta liste est vide. Dans les résultats d'un scan, clique sur le panier d'une opportunité pour l'ajouter." />
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto p-4">
          <div className="mb-4 grid grid-cols-2 gap-3 lg:grid-cols-4">
            <Total label="Reste à acheter" value={`${String(left.length)} objets`} />
            <Total label="Serveurs à visiter" value={String(worlds)} />
            <Total label="Budget" value={`${gil(sum(left, (i) => i.cost))} gils`} />
            <Total
              label={`Bénéfice attendu · marge ${percent(
                margin(
                  sum(left, (i) => i.profit),
                  sum(left, (i) => i.cost),
                ),
              )}`}
              value={`+${gil(sum(left, (i) => i.profit))} gils`}
              accent
            />
          </div>
          {oldest !== null && (
            <p className="mb-3 flex items-center gap-1.5 text-[11px] text-muted-foreground">
              <ShoppingCartIcon className="size-3" />
              Prix relevés lors du scan (le plus ancien {ago(oldest)}) : vérifie-les sur place, une
              annonce peut être partie entre-temps.
            </p>
          )}
          <div className="flex flex-col gap-6">
            {groups.map((d) => (
              <div key={d.dc} className="flex flex-col gap-2">
                <h2 className="flex items-baseline gap-2 text-sm font-semibold">
                  {d.dc}
                  <span className="text-xs font-normal text-muted-foreground tabular-nums">
                    {gil(d.cost)} à dépenser ·{" "}
                    <span className="text-emerald-500">+{gil(d.profit)}</span>
                  </span>
                </h2>
                <div className="grid gap-3 xl:grid-cols-2">
                  {d.worlds.map((w) => (
                    <WorldCard key={w.world} g={w} />
                  ))}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
