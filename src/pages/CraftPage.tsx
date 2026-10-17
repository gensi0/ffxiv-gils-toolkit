import { Fragment, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useQueryClient } from "@tanstack/react-query";
import {
  ChevronDownIcon,
  ChevronRightIcon,
  HammerIcon,
  SettingsIcon,
  ShoppingCartIcon,
  ArrowDownIcon,
  ArrowUpIcon,
} from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Progress } from "@/components/ui/progress";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { EmptyState, PageHeader } from "@/components/Page";
import { ItemIcon } from "@/components/market";
import {
  api,
  CRAFT_JOBS,
  errorMessage,
  GATHER_JOBS,
  EVENTS,
  type CraftNode,
  type CraftResults,
  type CraftRow,
  type ItemInfo,
  type World,
} from "@/lib/api";
import { ago, decimal, gil, percent } from "@/lib/format";
import { useSettings, useWorlds } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { useCraftOptions, type CraftSortKey } from "@/store/craftOptions";
import { useNav } from "@/store/nav";
import { useShoppingList, type CraftIngredient } from "@/store/shoppingList";

type Items = Record<string, ItemInfo>;

const itemName = (items: Items, id: number) => items[String(id)]?.name ?? `Objet n°${String(id)}`;

/** Achats aux prix réels (annonces), pour les fabrications conseillées. */
function realIngredients(
  row: CraftRow,
  items: Items,
  worlds: World[],
  homeRegion: string | undefined,
): CraftIngredient[] {
  return row.purchases.map((p) => {
    const info = items[String(p.item)];
    const region = worlds.find((w) => w.id === p.worldId)?.region;
    return {
      itemId: p.item,
      name: info?.name ?? `Objet n°${String(p.item)}`,
      iconPath: info?.iconPath ?? null,
      worldId: p.worldId,
      world: p.world,
      dc: p.dc,
      crossRegion: region !== undefined && region !== homeRegion,
      unitCost: p.cost / p.units,
      units: p.units,
    };
  });
}

/** Tous les composants non fabriqués de `crafts` fabrications (pour le journal). */
function consumed(tree: CraftNode, crafts: number) {
  const out = new Map<number, { itemId: number; quantity: number; unitCost: number }>();
  const walk = (n: CraftNode) => {
    if (n.source.kind === "craft") {
      n.children.forEach(walk);
      return;
    }
    const prev = out.get(n.item);
    if (prev) prev.quantity += n.quantity * crafts;
    else out.set(n.item, { itemId: n.item, quantity: n.quantity * crafts, unitCost: n.unitCost });
  };
  walk(tree);
  return [...out.values()];
}

/** Achats d'un arbre pour `crafts` fabrications, regroupés par objet et serveur. */
function ingredients(
  tree: CraftNode,
  crafts: number,
  items: Items,
  worlds: World[],
  homeRegion: string | undefined,
): CraftIngredient[] {
  const out = new Map<string, CraftIngredient>();
  const walk = (n: CraftNode) => {
    if (n.source.kind === "craft") {
      n.children.forEach(walk);
      return;
    }
    if (n.source.kind !== "buy") return;
    const { world, worldId, dc } = n.source;
    const key = `${String(n.item)}-${String(worldId)}`;
    const prev = out.get(key);
    const units = n.quantity * crafts;
    if (prev) {
      prev.units += units;
      return;
    }
    const info = items[String(n.item)];
    const region = worlds.find((w) => w.id === worldId)?.region;
    out.set(key, {
      itemId: n.item,
      name: info?.name ?? `Objet n°${String(n.item)}`,
      iconPath: info?.iconPath ?? null,
      worldId,
      world,
      dc,
      crossRegion: region !== undefined && region !== homeRegion,
      unitCost: n.unitCost,
      units,
    });
  };
  walk(tree);
  return [...out.values()].map((g) => ({ ...g, units: Math.ceil(g.units - 1e-9) }));
}

function TreeNode({ n, items, depth }: { n: CraftNode; items: Items; depth: number }) {
  const info = items[String(n.item)];
  return (
    <>
      <div className="flex items-center gap-2 py-0.5" style={{ paddingLeft: depth * 18 }}>
        <ItemIcon path={info?.iconPath ?? null} className="size-4" />
        <span className="text-muted-foreground tabular-nums">{decimal(n.quantity)} ×</span>
        <span>{itemName(items, n.item)}</span>
        <span className="text-muted-foreground">
          {n.source.kind === "buy" && (
            <>
              acheter sur <span className="text-foreground">{n.source.world}</span> à{" "}
              {gil(n.unitCost)} / u
            </>
          )}
          {n.source.kind === "craft" && (
            <>
              <span className="text-sky-500">fabriquer</span> ({CRAFT_JOBS[n.source.job]} niv.{" "}
              {n.source.level}) · revient à {gil(n.unitCost)} / u
            </>
          )}
          {n.source.kind === "npc" && (
            <>
              acheter chez <span className="text-foreground">{n.source.vendor}</span> (PNJ) à{" "}
              {gil(n.unitCost)} / u
            </>
          )}
          {n.source.kind === "gather" && (
            <>
              <span className="text-emerald-500">récolter</span> ({GATHER_JOBS[n.source.job]} niv.{" "}
              {n.source.level})
            </>
          )}
          {n.source.kind === "missing" && <span className="text-red-500">introuvable</span>}
        </span>
      </div>
      {n.children.map((c, i) => (
        <TreeNode key={`${String(c.item)}-${String(i)}`} n={c} items={items} depth={depth + 1} />
      ))}
    </>
  );
}

function Detail({ row, items }: { row: CraftRow; items: Items }) {
  const [crafts, setCrafts] = useState(String(row.suggestedCrafts));
  const addIngredients = useShoppingList((s) => s.addIngredients);
  const worlds = useWorlds();
  const settings = useSettings();
  const n = Math.max(1, Number.parseInt(crafts, 10) || 1);
  const home = worlds.data?.find((w) => w.id === settings.data?.homeWorldId);
  const real = row.realPrices && n === row.suggestedCrafts && row.purchases.length > 0;
  const list = real
    ? realIngredients(row, items, worlds.data ?? [], home?.region)
    : ingredients(row.tree, n, items, worlds.data ?? [], home?.region);
  const [recording, setRecording] = useState(false);
  const recordCraft = () => {
    setRecording(true);
    api
      .journalRecordCraft(row.item.id, n * row.amount, consumed(row.tree, n))
      .then((unit) => {
        toast.success(
          `${gil(n * row.amount)} × ${row.item.name} en stock à ${gil(unit)} gils l'unité : la marge suivra dans Mes ventes.`,
        );
      })
      .catch((e: unknown) => toast.error(errorMessage(e)))
      .finally(() => {
        setRecording(false);
      });
  };
  const total = list.reduce((t, g) => t + g.units * g.unitCost, 0);
  return (
    <div className="flex flex-col gap-3 bg-muted/30 px-4 py-3">
      <div className="text-[11px]">
        <TreeNode n={row.tree} items={items} depth={0} />
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Label htmlFor={`crafts-${String(row.recipeId)}`} className="text-xs">
          Fabrications
        </Label>
        <Input
          id={`crafts-${String(row.recipeId)}`}
          inputMode="numeric"
          className="h-7 w-16 text-xs"
          value={crafts}
          onChange={(e) => {
            setCrafts(e.target.value);
          }}
        />
        <span className="text-muted-foreground">
          → {gil(n * row.amount)} unités à vendre · {list.length} achat{list.length > 1 ? "s" : ""}{" "}
          pour {gil(total)} gils · bénéfice estimé{" "}
          <span className="text-emerald-500">+{gil(row.unitProfit * n * row.amount)}</span>
        </span>
        <Button
          size="xs"
          className="ml-auto"
          disabled={list.length === 0}
          onClick={() => {
            addIngredients(row.item.name, list);
            toast.success(`Ingrédients de ${row.item.name} ajoutés à la liste de courses.`);
          }}
        >
          <ShoppingCartIcon />
          Ajouter les ingrédients à la liste
        </Button>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button size="xs" variant="outline" disabled={recording} onClick={recordCraft}>
              <HammerIcon />
              J'ai fabriqué
            </Button>
          </TooltipTrigger>
          <TooltipContent className="max-w-64">
            Sort les ingrédients de ton stock (achats notés) et met le résultat en stock à son coût
            réel, pour suivre sa marge dans Mes ventes et le journal des gains.
          </TooltipContent>
        </Tooltip>
      </div>
      <p className="text-[11px] text-muted-foreground">
        Conseillé : {row.suggestedCrafts} fabrication{row.suggestedCrafts > 1 ? "s" : ""}, soit ce
        qui se vend sur ton horizon de vente. Les prix d'achat sont ceux de l'annonce la moins chère
        : vérifie les quantités disponibles.
      </p>
    </div>
  );
}

function Tags({ row }: { row: CraftRow }) {
  return (
    <span className="flex gap-1">
      {row.insufficient && (
        <span className="rounded bg-red-500/15 px-1 text-[10px] text-red-500">Stock HV limité</span>
      )}
      {row.masterBook && (
        <span className="rounded bg-amber-500/15 px-1 text-[10px] text-amber-500">Livre</span>
      )}
      {row.specialist && (
        <span className="rounded bg-violet-500/15 px-1 text-[10px] text-violet-500">
          Spécialiste
        </span>
      )}
      {row.expert && (
        <span className="rounded bg-red-500/15 px-1 text-[10px] text-red-500">Expert</span>
      )}
    </span>
  );
}

/** En-tête cliquable : trie le tableau (ordre gardé entre deux lancements). */
function SortTh(p: {
  k: CraftSortKey;
  align?: "left" | "right";
  className?: string;
  hint?: string;
  children: React.ReactNode;
}) {
  const sort = useCraftOptions((s) => s.sort);
  const set = useCraftOptions((s) => s.set);
  const active = sort.key === p.k;
  const label = (
    <button
      type="button"
      className={cn(
        "inline-flex items-center gap-1 hover:text-foreground",
        active && "text-foreground",
      )}
      onClick={() => {
        // Premier clic : décroissant pour les nombres, alphabétique pour le texte.
        set({ sort: { key: p.k, desc: active ? !sort.desc : p.k !== "item" && p.k !== "job" } });
      }}
    >
      {p.children}
      {active &&
        (sort.desc ? <ArrowDownIcon className="size-3" /> : <ArrowUpIcon className="size-3" />)}
    </button>
  );
  return (
    <th
      className={cn(
        "px-2 font-normal",
        p.align === "left" ? "text-left" : "text-right",
        p.className,
      )}
    >
      {p.hint ? (
        <Tooltip>
          <TooltipTrigger asChild>{label}</TooltipTrigger>
          <TooltipContent>{p.hint}</TooltipContent>
        </Tooltip>
      ) : (
        label
      )}
    </th>
  );
}

export function CraftPage() {
  const opts = useCraftOptions();
  const qc = useQueryClient();
  const settings = useSettings();
  const setPage = useNav((s) => s.setPage);
  const openItem = useNav((s) => s.openItem);
  const [results, setResults] = useState<CraftResults | null>(null);
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [open, setOpen] = useState<number | null>(null);
  const [job, setJob] = useState<number | null>(null);

  useEffect(() => {
    void api.craftLast().then((r) => {
      if (r) setResults(r);
    });
    const un = listen<{ done: number; total: number }>(EVENTS.craftProgress, (e) => {
      setProgress(e.payload);
    });
    return () => {
      void un.then((f) => {
        f();
      });
    };
  }, []);

  const levels = settings.data?.crafterLevels ?? [];
  const configured = levels.some((l) => l > 0);

  const run = () => {
    setRunning(true);
    setProgress(null);
    api
      .craftAnalyze({
        allowExpert: opts.allowExpert,
        allowMaster: opts.allowMaster,
        includeSpecialist: opts.includeSpecialist,
        gatherMyself: opts.gatherMyself,
      })
      .then((r) => {
        setResults(r);
        setOpen(null);
        void qc.invalidateQueries({ queryKey: ["craft", "last"] });
      })
      .catch((e: unknown) => toast.error(errorMessage(e)))
      .finally(() => {
        setRunning(false);
      });
  };

  const { key: sortKey, desc } = opts.sort;
  const rows = (results?.rows ?? [])
    .filter((r) => job === null || r.job === job)
    .sort((a, b) => {
      const c =
        sortKey === "item"
          ? a.item.name.localeCompare(b.item.name, "fr")
          : sortKey === "job"
            ? a.job - b.job || a.level - b.level
            : a[sortKey] - b[sortKey];
      return desc ? -c : c;
    });

  const gatherLevels = settings.data?.gathererLevels ?? [];
  const canGather = gatherLevels.some((l) => l > 0);
  const option = (
    key: "allowMaster" | "includeSpecialist" | "allowExpert" | "gatherMyself",
    label: string,
    disabled = false,
  ) => (
    <div className="flex items-center gap-1.5">
      <Checkbox
        id={key}
        disabled={disabled}
        checked={opts[key] && !disabled}
        onCheckedChange={(c) => {
          opts.set({ [key]: c === true });
        }}
      />
      <Label htmlFor={key} className="text-xs">
        {label}
      </Label>
    </div>
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader title="Artisanat">
        {results && (
          <span className="text-[11px] text-muted-foreground">
            analyse {ago(results.computedAtMs)} · {gil(results.recipesConsidered)} recettes
          </span>
        )}
        <Button size="sm" disabled={running || !configured} onClick={run}>
          <HammerIcon className={cn(running && "animate-pulse")} />
          {running ? "Analyse…" : "Analyser"}
        </Button>
      </PageHeader>

      {!configured ? (
        <EmptyState
          message="Importe les niveaux de tes métiers d'artisan depuis le Lodestone (ou saisis-les) dans les Paramètres."
          action={
            <Button
              size="sm"
              variant="outline"
              onClick={() => {
                setPage("settings");
              }}
            >
              <SettingsIcon />
              Ouvrir les paramètres
            </Button>
          }
        />
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-4 text-xs">
          <div className="flex flex-wrap items-center gap-4">
            {option("allowMaster", "Recettes de livres de maître")}
            {option("includeSpecialist", "Recettes de spécialiste")}
            {option("allowExpert", "Recettes expertes")}
            <Tooltip>
              <TooltipTrigger asChild>
                <span>{option("gatherMyself", "Je récolte moi-même", !canGather)}</span>
              </TooltipTrigger>
              <TooltipContent className="max-w-64">
                {canGather
                  ? "Les composants récoltables à ton niveau (Mineur, Botaniste, Pêcheur) comptent gratuits : seul ton temps y passe."
                  : "Renseigne tes niveaux de récolte dans les Paramètres (import Lodestone)."}
              </TooltipContent>
            </Tooltip>
            <span className="text-[11px] text-muted-foreground">
              Niveaux :{" "}
              {CRAFT_JOBS.map((j, i) => `${j.slice(0, 3)}. ${String(levels[i] ?? 0)}`).join(" · ")}
            </span>
          </div>
          <p className="text-[11px] text-muted-foreground">
            Pour chaque objet que tu sais fabriquer : composants au moins cher dans tes régions
            (achetés, ou fabriqués si c'est moins cher), vente en qualité normale sur ton serveur,
            taxe déduite. Mêmes seuils que le scan (ventes par jour, marge minimale, fraîcheur). Les
            premiers résultats sont recalculés sur les vraies annonces, pour les quantités
            conseillées.
          </p>
          {results && !results.npcPrices && (
            <p className="text-[11px] text-amber-500">
              Les prix des marchands PNJ ne sont pas pris en compte : prépare les données dans
              l'onglet Monnaies (une fois), puis relance l'analyse.
            </p>
          )}

          {running && (
            <div className="flex items-center gap-3">
              <Progress
                value={progress ? (progress.done / Math.max(1, progress.total)) * 100 : 5}
                className="h-1.5 w-64"
              />
              <span className="text-muted-foreground">
                {progress
                  ? `${String(progress.done)} / ${String(progress.total)} blocs de prix`
                  : "Chargement des recettes…"}
              </span>
            </div>
          )}

          {results && (
            <div className="flex flex-wrap gap-1">
              <Button
                size="xs"
                variant={job === null ? "secondary" : "ghost"}
                onClick={() => {
                  setJob(null);
                }}
              >
                Tous ({results.rows.length})
              </Button>
              {CRAFT_JOBS.map((j, i) => {
                const count = results.rows.filter((r) => r.job === i).length;
                if (count === 0) return null;
                return (
                  <Button
                    key={j}
                    size="xs"
                    variant={job === i ? "secondary" : "ghost"}
                    onClick={() => {
                      setJob(i);
                    }}
                  >
                    {j} ({count})
                  </Button>
                );
              })}
            </div>
          )}

          {!results && !running ? (
            <p className="text-muted-foreground">Lance une analyse pour voir quoi fabriquer.</p>
          ) : results && rows.length === 0 ? (
            <p className="text-muted-foreground">
              Aucun objet rentable avec ces réglages. Essaie d'inclure d'autres recettes ou de
              baisser la marge minimale dans les paramètres.
            </p>
          ) : (
            results && (
              <div className="rounded-md border">
                <table className="w-full">
                  <thead className="text-[11px] text-muted-foreground">
                    <tr>
                      <th className="w-6" />
                      <SortTh k="item" align="left" className="py-1.5">
                        Objet
                      </SortTh>
                      <SortTh k="job" align="left">
                        Métier
                      </SortTh>
                      <SortTh k="unitCost">Coût / u</SortTh>
                      <SortTh k="sellPrice">Vente / u</SortTh>
                      <SortTh k="unitProfit">Bénéfice / u</SortTh>
                      <SortTh k="margin">Marge</SortTh>
                      <SortTh k="unitsPerDay">Ventes / jour</SortTh>
                      <SortTh k="profitPerDay" hint="Bénéfice par unité × ventes par jour.">
                        Bénéfice / jour
                      </SortTh>
                    </tr>
                  </thead>
                  <tbody>
                    {rows.map((r) => (
                      <Fragment key={r.recipeId}>
                        <tr
                          className={cn(
                            "cursor-pointer border-t border-border/50 hover:bg-muted/40",
                            open === r.recipeId && "bg-muted/40",
                          )}
                          onClick={() => {
                            setOpen(open === r.recipeId ? null : r.recipeId);
                          }}
                        >
                          <td className="pl-2 text-muted-foreground">
                            {open === r.recipeId ? (
                              <ChevronDownIcon className="size-3.5" />
                            ) : (
                              <ChevronRightIcon className="size-3.5" />
                            )}
                          </td>
                          <td className="h-9 px-2">
                            <span className="flex items-center gap-2">
                              <ItemIcon path={r.item.iconPath} className="size-5" />
                              <button
                                type="button"
                                className="max-w-64 truncate hover:underline"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  openItem(r.item.id);
                                }}
                              >
                                {r.item.name}
                              </button>
                              {r.amount > 1 && (
                                <span className="text-muted-foreground">×{r.amount}</span>
                              )}
                              <Tags row={r} />
                            </span>
                          </td>
                          <td className="px-2 text-muted-foreground">
                            {CRAFT_JOBS[r.job]} {r.level}
                          </td>
                          <td className="px-2 text-right tabular-nums">
                            {r.realPrices ? (
                              <Tooltip>
                                <TooltipTrigger className="cursor-help underline decoration-emerald-500/60 decoration-dotted">
                                  {gil(r.unitCost)}
                                </TooltipTrigger>
                                <TooltipContent>
                                  Prix réels des annonces pour {r.suggestedCrafts} fabrication
                                  {r.suggestedCrafts > 1 ? "s" : ""}.
                                </TooltipContent>
                              </Tooltip>
                            ) : (
                              gil(r.unitCost)
                            )}
                          </td>
                          <td className="px-2 text-right tabular-nums">{gil(r.sellPrice)}</td>
                          <td className="px-2 text-right font-medium text-emerald-500 tabular-nums">
                            +{gil(r.unitProfit)}
                          </td>
                          <td className="px-2 text-right tabular-nums">{percent(r.margin)}</td>
                          <td className="px-2 text-right tabular-nums">{decimal(r.unitsPerDay)}</td>
                          <td className="px-2 text-right font-medium tabular-nums">
                            {gil(r.profitPerDay)}
                          </td>
                        </tr>
                        {open === r.recipeId && (
                          <tr>
                            <td colSpan={9}>
                              <Detail row={r} items={results.items} />
                            </td>
                          </tr>
                        )}
                      </Fragment>
                    ))}
                  </tbody>
                </table>
              </div>
            )
          )}
        </div>
      )}
    </div>
  );
}
