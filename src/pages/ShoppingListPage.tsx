import { Fragment, useMemo, useState } from "react";
import { ShoppingCartIcon, Trash2Icon, XIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { toast } from "sonner";
import { api, errorMessage } from "@/lib/api";
import { useVentesAction } from "@/lib/queries";
import { EmptyState, PageHeader } from "@/components/Page";
import { LotChips, LotsTooltip } from "@/components/Lots";
import { ItemIcon, QualityTag } from "@/components/market";
import { MyStockBadge } from "@/components/MyStockBadge";
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

/** Saisie du prix réellement payé, enregistré dans le journal des gains. */
function PurchaseForm({ i, onDone }: { i: ShoppingItem; onDone: () => void }) {
  const setBought = useShoppingList((s) => s.setBought);
  const [qty, setQty] = useState(String(i.units));
  const [total, setTotal] = useState(String(Math.round(i.cost)));
  const q = Number.parseInt(qty.replace(/\s/g, ""), 10);
  const t = Number.parseInt(total.replace(/\s/g, ""), 10);
  const valid = q > 0 && t >= 0;
  const add = useVentesAction(() =>
    api.journalAddPurchase({
      itemId: i.itemId,
      hq: i.quality === "HQ",
      quantity: q,
      unitCost: t / q,
      sourceWorld: i.world,
    }),
  );
  return (
    <form
      className="flex flex-wrap items-center gap-2 bg-muted/40 px-3 py-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (!valid) return;
        add.mutate(undefined, {
          onSuccess: (id) => {
            setBought(i.key, true, id as number);
            onDone();
            toast.success("Achat noté dans le journal des gains.");
          },
          onError: (err) => toast.error(errorMessage(err)),
        });
      }}
    >
      <span className="text-muted-foreground">Ce que tu as vraiment acheté :</span>
      <Input
        aria-label="Quantité achetée"
        inputMode="numeric"
        className="h-7 w-16 text-xs"
        value={qty}
        onChange={(e) => {
          setQty(e.target.value);
        }}
      />
      <span className="text-muted-foreground">unités pour</span>
      <Input
        aria-label="Total payé"
        inputMode="numeric"
        className="h-7 w-28 text-xs"
        value={total}
        onChange={(e) => {
          setTotal(e.target.value);
        }}
      />
      <span className="text-muted-foreground">gils (taxe comprise)</span>
      <Button type="submit" size="xs" disabled={!valid || add.isPending}>
        Noter l'achat
      </Button>
      <Button
        type="button"
        size="xs"
        variant="ghost"
        onClick={() => {
          setBought(i.key, true);
          onDone();
        }}
      >
        Cocher sans noter
      </Button>
    </form>
  );
}

function WorldCard({ g }: { g: WorldGroup }) {
  const { remove, setBought } = useShoppingList();
  const openItem = useNav((s) => s.openItem);
  const [recording, setRecording] = useState<string | null>(null);
  const delPurchase = useVentesAction((id: number) => api.journalDeletePurchase(id));
  const onCheck = (i: ShoppingItem, checked: boolean) => {
    if (checked) {
      setRecording(i.key);
      return;
    }
    if (i.purchaseId !== undefined) {
      if (!window.confirm("Décocher supprime aussi l'achat noté dans le journal. Continuer ?"))
        return;
      delPurchase.mutate(i.purchaseId);
    }
    setBought(i.key, false);
  };
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
            <Fragment key={i.key}>
              <tr className="border-t border-border/50 first:border-t-0">
                <td className="w-8 pl-3">
                  <Checkbox
                    checked={i.bought || recording === i.key}
                    aria-label="Acheté"
                    onCheckedChange={(c) => {
                      onCheck(i, c === true);
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
                  {!i.bought && !i.forCraft && <MyStockBadge itemId={i.itemId} />}
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
                {i.forCraft ? (
                  <td colSpan={2} className="px-2 text-right text-muted-foreground">
                    <span className="block max-w-56 truncate" title={i.forCraft.join(", ")}>
                      pour fabriquer {i.forCraft.join(", ")}
                    </span>
                  </td>
                ) : (
                  <>
                    <td className="px-2 text-right text-muted-foreground tabular-nums">
                      revente {gil(i.sellPrice)}
                    </td>
                    <td className="px-2 text-right text-emerald-500 tabular-nums">
                      +{gil(i.profit)}
                    </td>
                  </>
                )}
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
              {recording === i.key && (
                <tr>
                  <td colSpan={6}>
                    <PurchaseForm
                      i={i}
                      onDone={() => {
                        setRecording(null);
                      }}
                    />
                  </td>
                </tr>
              )}
            </Fragment>
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
