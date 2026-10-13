import { useState } from "react";
import { CheckIcon, PencilIcon, RefreshCwIcon, SettingsIcon, XIcon } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { EmptyState, PageHeader } from "@/components/Page";
import { ItemIcon, QualityTag } from "@/components/market";
import { api, errorMessage, type MyListing } from "@/lib/api";
import { ago, gil } from "@/lib/format";
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

function ListingRow({ l }: { l: MyListing }) {
  const [editing, setEditing] = useState(false);
  const setPrice = useVentesAction((p: number) => api.ventesSetPrice(l.listingId, p));
  const close = useVentesAction((sold: boolean) => api.ventesClose(l.listingId, sold, null));
  const shown = l.expectedPrice ?? l.price;
  return (
    <tr className="border-t border-border/50">
      <td className="h-9 px-3">
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
        {l.undercutPrice !== null ? (
          <span className="flex items-center gap-1.5 text-red-500">
            Devancé à {gil(l.undercutPrice)}
            {l.suggestedPrice !== null && (
              <span className="text-muted-foreground">· conseillé {gil(l.suggestedPrice)}</span>
            )}
          </span>
        ) : (
          <span className="text-emerald-500">Le moins cher</span>
        )}
      </td>
      <td className="px-2 text-right text-muted-foreground">{ago(l.firstSeen)}</td>
      <td className="px-2">
        <span className="flex justify-end gap-1">
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
  const untrack = useVentesAction((id: number) => api.ventesTrack(id, false));
  const d = data.data;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader title="Mes ventes">
        {d?.lastPollMs != null && (
          <span className="text-[11px] text-muted-foreground">relevé {ago(d.lastPollMs)}</span>
        )}
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
            L'app relève toutes les 3 minutes les objets que tu as achetés, mis en vente ou suivis.
            Universalis n'est à jour que lorsqu'un joueur consulte l'HV : une vente peut apparaître
            avec du retard.
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
            hint="Si tu changes un prix en jeu, note-le avec « Prix modifié » : le suivi reste juste en attendant Universalis."
          >
            {d.listings.length === 0 ? (
              <p className="text-muted-foreground">
                Aucune annonce de tes servants vue sur ton serveur pour les objets suivis.
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
                      <th className="px-2 text-left font-normal">Concurrence</th>
                      <th className="px-2 text-right font-normal">En vente depuis</th>
                      <th />
                    </tr>
                  </thead>
                  <tbody>
                    {d.listings.map((l) => (
                      <ListingRow key={l.listingId} l={l} />
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
