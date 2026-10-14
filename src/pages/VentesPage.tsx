import { useState } from "react";
import {
  CheckIcon,
  PencilIcon,
  RefreshCwIcon,
  SearchIcon,
  SettingsIcon,
  XIcon,
} from "lucide-react";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { EmptyState, PageHeader } from "@/components/Page";
import { ItemIcon, QualityTag } from "@/components/market";
import { api, errorMessage, type MyListing, type VentesOverview } from "@/lib/api";
import { ago, gil, percent } from "@/lib/format";
import { useVentes, useVentesAction } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { useNav } from "@/store/nav";

function ItemName({ l }: { l: { item: MyListing["item"]; hq: boolean } }) {
  const openItem = useNav((s) => s.openItem);
  return (
    <button
      type="button"
      className="flex items-center gap-2 text-left hover:underline"
      onClick={() => {
        openItem(l.item.id);
      }}
    >
      <ItemIcon path={l.item.iconPath} className="size-5" />
      <span className="max-w-60 truncate">{l.item.name}</span>
      <QualityTag quality={l.hq ? "HQ" : "NQ"} />
    </button>
  );
}

function Section({
  title,
  hint,
  children,
}: {
  title: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <section className="flex flex-col gap-2">
      <h2 className="text-sm font-semibold">{title}</h2>
      {hint && <p className="-mt-1 text-[11px] text-muted-foreground">{hint}</p>}
      {children}
    </section>
  );
}

/** Saisie d'un prix (modification en jeu ou prix de vente confirmé). */
function PriceInput(props: {
  initial: number;
  label: string;
  onSubmit: (price: number) => void;
  onCancel: () => void;
}) {
  const [value, setValue] = useState(String(props.initial));
  const n = Number.parseInt(value.replace(/\s/g, ""), 10);
  const valid = Number.isFinite(n) && n > 0;
  return (
    <form
      className="flex items-center gap-1"
      onSubmit={(e) => {
        e.preventDefault();
        if (valid) props.onSubmit(n);
      }}
    >
      <Input
        autoFocus
        inputMode="numeric"
        aria-label={props.label}
        className="h-7 w-24 text-xs"
        value={value}
        onChange={(e) => {
          setValue(e.target.value);
        }}
      />
      <Button type="submit" size="icon-sm" variant="outline" disabled={!valid} aria-label="Valider">
        <CheckIcon />
      </Button>
      <Button
        type="button"
        size="icon-sm"
        variant="ghost"
        onClick={props.onCancel}
        aria-label="Annuler"
      >
        <XIcon />
      </Button>
    </form>
  );
}

const ordinal = (n: number) => (n === 1 ? "1re" : `${String(n)}e`);

/** Marge unitaire nette (taxe déduite) au prix affiché, si le coût est connu. */
function margin(l: MyListing, tax: number): { gil: number; pct: number } | null {
  if (l.unitCost === null || l.unitCost <= 0) return null;
  const net = (l.expectedPrice ?? l.price) * (1 - tax) - l.unitCost;
  return { gil: net, pct: net / l.unitCost };
}

function RankCell({ l, top }: { l: MyListing; top: number }) {
  if (l.rank === null) return <span className="text-muted-foreground">—</span>;
  const inTop = l.rank <= top;
  const mine = l.expectedPrice ?? l.price;
  return (
    <span className="flex flex-col leading-tight">
      <span className={cn("font-medium", inTop ? "text-emerald-500" : "text-red-500")}>
        {ordinal(l.rank)}
        <span className="font-normal text-muted-foreground"> sur {l.listingCount ?? "?"}</span>
      </span>
      {l.cheapestOther !== null && l.cheapestOther < mine && (
        <span className="text-[10px] text-muted-foreground">
          moins cher : {gil(l.cheapestOther)} (−{gil(mine - l.cheapestOther)})
        </span>
      )}
    </span>
  );
}

function CostCell({ l, tax }: { l: MyListing; tax: number }) {
  const [editing, setEditing] = useState(false);
  const add = useVentesAction((unitCost: number) =>
    api.journalAddPurchase({
      itemId: l.item.id,
      hq: l.hq,
      quantity: l.quantity,
      unitCost,
      sourceWorld: null,
    }),
  );
  const m = margin(l, tax);
  if (editing) {
    return (
      <PriceInput
        initial={0}
        label="Coût unitaire payé"
        onCancel={() => {
          setEditing(false);
        }}
        onSubmit={(c) => {
          add.mutate(c, {
            onSuccess: () => {
              setEditing(false);
              toast.success("Coût noté dans le journal des gains.");
            },
            onError: (e) => toast.error(errorMessage(e)),
          });
        }}
      />
    );
  }
  if (l.unitCost === null) {
    return (
      <Button
        size="xs"
        variant="ghost"
        className="text-muted-foreground"
        onClick={() => {
          setEditing(true);
        }}
      >
        Saisir le coût
      </Button>
    );
  }
  return (
    <span className="flex items-center gap-1.5">
      <Tooltip>
        <TooltipTrigger asChild>
          <span className="rounded bg-sky-500/15 px-1.5 py-0.5 text-[10px] font-medium text-sky-500">
            Acheté
          </span>
        </TooltipTrigger>
        <TooltipContent>Coût moyen payé : {gil(l.unitCost)} gils l'unité</TooltipContent>
      </Tooltip>
      {m && (
        <span className={cn("tabular-nums", m.gil >= 0 ? "text-emerald-500" : "text-red-500")}>
          {m.gil >= 0 ? "+" : ""}
          {gil(m.gil)} ({percent(m.pct)})
        </span>
      )}
    </span>
  );
}

function ListingRow({ l, top, tax }: { l: MyListing; top: number; tax: number }) {
  const [editing, setEditing] = useState(false);
  const setPrice = useVentesAction((p: number) => api.ventesSetPrice(l.listingId, p));
  const close = useVentesAction((sold: boolean) => api.ventesClose(l.listingId, sold, null));
  const shown = l.expectedPrice ?? l.price;
  return (
    <tr className="border-t border-border/50">
      <td className="h-10 px-3">
        <ItemName l={l} />
      </td>
      <td className="px-2 text-muted-foreground">{l.retainer}</td>
      <td className="px-2 text-right tabular-nums">{gil(l.quantity)}</td>
      <td className="px-2 text-right tabular-nums">
        {editing ? (
          <PriceInput
            initial={l.suggestedPrice ?? shown}
            label="Nouveau prix"
            onCancel={() => {
              setEditing(false);
            }}
            onSubmit={(p) => {
              setPrice.mutate(p, {
                onSuccess: () => {
                  setEditing(false);
                  toast.success("Prix noté, en attente de confirmation par Universalis.");
                },
                onError: (e) => toast.error(errorMessage(e)),
              });
            }}
          />
        ) : (
          <span className="flex items-center justify-end gap-1">
            {l.expectedPrice !== null && (
              <Tooltip>
                <TooltipTrigger asChild>
                  <span className="text-[10px] text-amber-500">en attente</span>
                </TooltipTrigger>
                <TooltipContent>
                  Prix modifié dans l'app ({gil(l.expectedPrice)}), Universalis affiche encore{" "}
                  {gil(l.price)}.
                </TooltipContent>
              </Tooltip>
            )}
            <span className="font-medium">{gil(shown)}</span>
          </span>
        )}
      </td>
      <td className="px-2">
        <RankCell l={l} top={top} />
      </td>
      <td className="px-2">
        <CostCell l={l} tax={tax} />
      </td>
      <td className="px-2 text-right text-muted-foreground">{ago(l.firstSeen)}</td>
      <td className="px-2">
        <span className="flex justify-end gap-1">
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                size="xs"
                variant="outline"
                onClick={() => {
                  setEditing(true);
                }}
              >
                <PencilIcon />
                Prix modifié
              </Button>
            </TooltipTrigger>
            {l.suggestedPrice !== null && (
              <TooltipContent>Prix conseillé : {gil(l.suggestedPrice)}</TooltipContent>
            )}
          </Tooltip>
          <Button
            size="xs"
            variant="ghost"
            disabled={close.isPending}
            onClick={() => {
              close.mutate(false, { onError: (e) => toast.error(errorMessage(e)) });
            }}
          >
            Retirée
          </Button>
        </span>
      </td>
    </tr>
  );
}

type Filter = "all" | "out" | "bought";

function PendingRow({ l }: { l: MyListing }) {
  const [selling, setSelling] = useState(false);
  const close = useVentesAction((a: { sold: boolean; price: number | null }) =>
    api.ventesClose(l.listingId, a.sold, a.price),
  );
  const onError = (e: unknown) => toast.error(errorMessage(e));
  return (
    <div className="flex flex-wrap items-center gap-3 rounded-md border border-amber-500/40 px-3 py-2 text-xs">
      <ItemName l={l} />
      <span className="text-muted-foreground tabular-nums">
        {gil(l.quantity)} × {gil(l.expectedPrice ?? l.price)} · {l.retainer}
      </span>
      <span className="text-muted-foreground">
        Disparue de l'HV {ago(l.lastSeen)} sans vente correspondante.
      </span>
      <span className="ml-auto flex items-center gap-1">
        {selling ? (
          <PriceInput
            initial={l.expectedPrice ?? l.price}
            label="Prix de vente unitaire"
            onCancel={() => {
              setSelling(false);
            }}
            onSubmit={(p) => {
              close.mutate({ sold: true, price: p }, { onError });
            }}
          />
        ) : (
          <>
            <Button
              size="xs"
              variant="outline"
              onClick={() => {
                setSelling(true);
              }}
            >
              Vendue
            </Button>
            <Button
              size="xs"
              variant="ghost"
              onClick={() => {
                close.mutate({ sold: false, price: null }, { onError });
              }}
            >
              Retirée
            </Button>
          </>
        )}
      </span>
    </div>
  );
}

export function VentesPage() {
  const data = useVentes();
  const setPage = useNav((s) => s.setPage);
  const refresh = useVentesAction(() => api.ventesRefresh());
  const discover = useVentesAction(() => api.ventesDiscover());
  const [filter, setFilter] = useState<Filter>("all");
  const [search, setSearch] = useState("");
  const untrack = useVentesAction((id: number) => api.ventesTrack(id, false));
  const d = data.data;
  const isOut = (l: MyListing) => l.rank !== null && l.rank > (d?.topRank ?? 1);
  const outCount = d?.listings.filter(isOut).length ?? 0;
  const q = search.trim().toLowerCase();
  // Hors top d'abord (du plus mal classé au mieux), puis par nom.
  const shown = (d?.listings ?? [])
    .filter((l) => filter !== "out" || isOut(l))
    .filter((l) => filter !== "bought" || l.unitCost !== null)
    .filter(
      (l) =>
        q === "" || l.item.name.toLowerCase().includes(q) || l.retainer.toLowerCase().includes(q),
    )
    .sort(
      (a, b) =>
        Number(isOut(b)) - Number(isOut(a)) ||
        (b.rank ?? 0) - (a.rank ?? 0) ||
        a.item.name.localeCompare(b.item.name, "fr"),
    );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader title="Mes ventes">
        {d?.lastPollMs != null && (
          <span className="text-[11px] text-muted-foreground">relevé {ago(d.lastPollMs)}</span>
        )}
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="outline"
              size="sm"
              disabled={discover.isPending || d?.discovering === true || d?.configured !== true}
              onClick={() => {
                discover.mutate(undefined, {
                  onSuccess: (o) => {
                    toast.success(
                      `${String((o as VentesOverview).listings.length)} annonces suivies.`,
                    );
                  },
                  onError: (e) => toast.error(errorMessage(e)),
                });
              }}
            >
              <SearchIcon
                className={cn((discover.isPending || d?.discovering) && "animate-pulse")}
              />
              {discover.isPending || d?.discovering ? "Recherche…" : "Chercher toutes mes annonces"}
            </Button>
          </TooltipTrigger>
          <TooltipContent className="max-w-64">
            Parcourt tout le marché de ton serveur (environ une minute). Fait automatiquement toutes
            les 30 minutes
            {d?.lastDiscoveryMs != null ? ` ; dernière recherche ${ago(d.lastDiscoveryMs)}` : ""}.
          </TooltipContent>
        </Tooltip>
        <Button
          variant="outline"
          size="sm"
          disabled={refresh.isPending || d?.configured !== true}
          onClick={() => {
            refresh.mutate(undefined, { onError: (e) => toast.error(errorMessage(e)) });
          }}
        >
          <RefreshCwIcon className={cn(refresh.isPending && "animate-spin")} />
          Relever maintenant
        </Button>
      </PageHeader>

      {data.isPending ? (
        <Skeleton className="m-4 h-40" />
      ) : !d?.configured ? (
        <EmptyState
          message="Renseigne les noms de tes servants dans les Paramètres pour que l'app reconnaisse tes annonces."
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
        <div className="flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto p-4 text-xs">
          <p className="text-[11px] text-muted-foreground">
            L'app retrouve toutes tes annonces en parcourant le marché de ton serveur toutes les 30
            minutes, puis les relève toutes les 3 minutes. Universalis n'est à jour que lorsqu'un
            joueur consulte l'objet à l'HV : une annonce ou une vente peut apparaître avec du
            retard.
          </p>

          {d.pending.length > 0 && (
            <Section
              title={`À confirmer (${String(d.pending.length)})`}
              hint="Ces annonces ont disparu sans vente identique dans l'historique : vendues ou retirées ?"
            >
              {d.pending.map((l) => (
                <PendingRow key={l.listingId} l={l} />
              ))}
            </Section>
          )}

          <Section
            title={`En vente (${String(d.listings.length)})`}
            hint="Toutes les annonces de tes servants sur ton serveur. Si tu changes un prix en jeu, note-le avec « Prix modifié » : le suivi reste juste en attendant Universalis."
          >
            <div className="flex flex-wrap items-center gap-2">
              <ToggleGroup
                type="single"
                size="sm"
                variant="outline"
                value={filter}
                onValueChange={(v) => {
                  if (v) setFilter(v as Filter);
                }}
              >
                <ToggleGroupItem value="all" className="px-3 text-xs">
                  Toutes
                </ToggleGroupItem>
                <ToggleGroupItem value="out" className="px-3 text-xs">
                  Hors top {d.topRank} ({outCount})
                </ToggleGroupItem>
                <ToggleGroupItem value="bought" className="px-3 text-xs">
                  Achetées pour revente
                </ToggleGroupItem>
              </ToggleGroup>
              <Input
                placeholder="Rechercher un objet ou un servant"
                className="h-8 w-64 text-xs"
                value={search}
                onChange={(e) => {
                  setSearch(e.target.value);
                }}
              />
            </div>
            {shown.length === 0 ? (
              <p className="text-muted-foreground">
                {d.listings.length === 0
                  ? d.discovering
                    ? "Recherche de tes annonces sur tout le serveur…"
                    : "Aucune annonce de tes servants trouvée sur ton serveur."
                  : "Aucune annonce pour ce filtre."}
              </p>
            ) : (
              <div className="rounded-md border">
                <table className="w-full">
                  <thead className="text-[11px] text-muted-foreground">
                    <tr>
                      <th className="px-3 py-1.5 text-left font-normal">Objet</th>
                      <th className="px-2 text-left font-normal">Servant</th>
                      <th className="px-2 text-right font-normal">Quantité</th>
                      <th className="px-2 text-right font-normal">Mon prix</th>
                      <th className="px-2 text-left font-normal">Rang</th>
                      <th className="px-2 text-left font-normal">Achat · marge unitaire</th>
                      <th className="px-2 text-right font-normal">En vente depuis</th>
                      <th />
                    </tr>
                  </thead>
                  <tbody>
                    {shown.map((l) => (
                      <ListingRow key={l.listingId} l={l} top={d.topRank} tax={d.taxRate} />
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </Section>

          {d.stock.length > 0 && (
            <Section
              title="Stock acheté"
              hint="Achats notés depuis la liste de courses, pas encore vendus. Leur coût est déduit à chaque vente."
            >
              <div className="flex flex-wrap gap-2">
                {d.stock.map((s) => (
                  <div
                    key={`${String(s.item.id)}-${String(s.hq)}`}
                    className="flex items-center gap-3 rounded-md border px-3 py-2"
                  >
                    <ItemName l={s} />
                    <span className="tabular-nums">
                      {gil(s.remaining)} u. · coût moyen {gil(s.avgCost)}
                    </span>
                  </div>
                ))}
              </div>
            </Section>
          )}

          {d.tracked.length > 0 && (
            <Section
              title="Objets suivis"
              hint="Ajoutés depuis la fiche d'un objet, pour suivre tes ventes même sans achat noté."
            >
              <div className="flex flex-wrap gap-2">
                {d.tracked.map((item) => (
                  <span
                    key={item.id}
                    className="flex items-center gap-2 rounded-md border px-2 py-1"
                  >
                    <ItemIcon path={item.iconPath} className="size-4" />
                    {item.name}
                    <button
                      type="button"
                      aria-label="Ne plus suivre"
                      className="text-muted-foreground hover:text-foreground"
                      onClick={() => {
                        untrack.mutate(item.id);
                      }}
                    >
                      <XIcon className="size-3" />
                    </button>
                  </span>
                ))}
              </div>
            </Section>
          )}
        </div>
      )}
    </div>
  );
}
