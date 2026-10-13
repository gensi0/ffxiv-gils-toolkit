import { useState } from "react";
import { Trash2Icon } from "lucide-react";
import { toast } from "sonner";
import { Skeleton } from "@/components/ui/skeleton";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { EmptyState, PageHeader } from "@/components/Page";
import { ItemIcon, QualityTag } from "@/components/market";
import { api, errorMessage, type JournalSale } from "@/lib/api";
import { gil, percent } from "@/lib/format";
import { useJournal, useVentesAction } from "@/lib/queries";
import { cn } from "@/lib/utils";

const DAY = 24 * 3_600_000;
const PERIODS = [
  { id: "7", label: "7 jours", days: 7 },
  { id: "30", label: "30 jours", days: 30 },
  { id: "365", label: "1 an", days: 365 },
] as const;

/** Montant net d'une vente (taxe de vente déduite). */
const net = (s: JournalSale) => s.unitPrice * s.quantity * (1 - s.taxRate);

/** Bénéfice d'une vente : seulement sur les unités dont le coût d'achat est connu. */
function profit(s: JournalSale): number | null {
  if (s.costUnits === 0) return null;
  return (net(s) * s.costUnits) / s.quantity - s.cost;
}

const date = (ms: number) =>
  new Date(ms).toLocaleString("fr-FR", {
    day: "2-digit",
    month: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });

function Total(props: { label: string; value: string; accent?: "good" | "bad"; hint?: string }) {
  const content = (
    <div className="flex flex-col rounded-md border px-3 py-2">
      <span className="text-[11px] text-muted-foreground">{props.label}</span>
      <span
        className={cn(
          "text-base font-semibold tabular-nums",
          props.accent === "good" && "text-emerald-500",
          props.accent === "bad" && "text-red-500",
        )}
      >
        {props.value}
      </span>
    </div>
  );
  if (!props.hint) return content;
  return (
    <Tooltip>
      <TooltipTrigger asChild>{content}</TooltipTrigger>
      <TooltipContent className="max-w-64">{props.hint}</TooltipContent>
    </Tooltip>
  );
}

function DeleteButton({ onClick, label }: { onClick: () => void; label: string }) {
  return (
    <button
      type="button"
      aria-label={label}
      className="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
      onClick={() => {
        if (window.confirm(`${label} ?`)) onClick();
      }}
    >
      <Trash2Icon className="size-3.5" />
    </button>
  );
}

export function JournalPage() {
  const [period, setPeriod] = useState<(typeof PERIODS)[number]["id"]>("30");
  const days = PERIODS.find((p) => p.id === period)?.days ?? 30;
  // Heure d'ouverture de la page, arrondie : la clé de requête reste stable.
  const [now] = useState(() => Math.floor(Date.now() / 3_600_000) * 3_600_000);
  const since = now - days * DAY;
  const data = useJournal(since);
  const delSale = useVentesAction((id: number) => api.journalDeleteSale(id));
  const delPurchase = useVentesAction((id: number) => api.journalDeletePurchase(id));
  const onError = (e: unknown) => toast.error(errorMessage(e));

  const sales = data.data?.sales ?? [];
  const purchases = (data.data?.purchases ?? []).filter((p) => p.boughtAt >= since);
  const revenue = sales.reduce((t, s) => t + net(s), 0);
  const known = sales.filter((s) => s.costUnits > 0);
  const totalProfit = known.reduce((t, s) => t + (profit(s) ?? 0), 0);
  const costOfSold = known.reduce((t, s) => t + s.cost, 0);
  const spent = purchases.reduce((t, p) => t + p.unitCost * p.quantity, 0);
  const noCost = sales.length - known.length;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader title="Journal des gains">
        <ToggleGroup
          type="single"
          size="sm"
          variant="outline"
          value={period}
          onValueChange={(v) => {
            if (v) setPeriod(v as typeof period);
          }}
        >
          {PERIODS.map((p) => (
            <ToggleGroupItem key={p.id} value={p.id} className="px-3 text-xs">
              {p.label}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </PageHeader>

      {data.isPending ? (
        <Skeleton className="m-4 h-40" />
      ) : sales.length === 0 && purchases.length === 0 ? (
        <EmptyState message="Rien sur cette période. Coche « acheté » dans ta liste de courses pour noter tes achats ; tes ventes sont détectées dans « Mes ventes »." />
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto p-4 text-xs">
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            <Total
              label="Bénéfice réel"
              value={`${totalProfit >= 0 ? "+" : ""}${gil(totalProfit)} gils`}
              accent={totalProfit >= 0 ? "good" : "bad"}
              hint="Ventes nettes (taxe déduite) moins le prix payé, sur les ventes dont le coût d'achat est connu."
            />
            <Total
              label="Marge"
              value={costOfSold > 0 ? percent(totalProfit / costOfSold) : "—"}
              hint="Bénéfice réel ÷ coût d'achat des unités vendues."
            />
            <Total
              label="Ventes nettes"
              value={`${gil(revenue)} gils`}
              hint="Taxe de vente déduite."
            />
            <Total label="Achats" value={`${gil(spent)} gils`} hint="Total payé sur la période." />
          </div>
          {noCost > 0 && (
            <p className="text-[11px] text-muted-foreground">
              {noCost} vente{noCost > 1 ? "s" : ""} sans coût d'achat connu (objet non noté depuis
              la liste de courses) : comptée{noCost > 1 ? "s" : ""} dans les ventes nettes, pas dans
              le bénéfice.
            </p>
          )}

          <section className="flex flex-col gap-2">
            <h2 className="text-sm font-semibold">Ventes ({sales.length})</h2>
            <div className="rounded-md border">
              <table className="w-full">
                <thead className="text-[11px] text-muted-foreground">
                  <tr>
                    <th className="px-3 py-1.5 text-left font-normal">Date</th>
                    <th className="px-2 text-left font-normal">Objet</th>
                    <th className="px-2 text-right font-normal">Quantité × prix</th>
                    <th className="px-2 text-right font-normal">Net</th>
                    <th className="px-2 text-right font-normal">Coût</th>
                    <th className="px-2 text-right font-normal">Bénéfice</th>
                    <th className="px-2 text-left font-normal">Source</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {sales.map((s) => {
                    const p = profit(s);
                    return (
                      <tr key={s.id} className="border-t border-border/50">
                        <td className="h-8 px-3 text-muted-foreground tabular-nums">
                          {date(s.soldAt)}
                        </td>
                        <td className="px-2">
                          <span className="flex items-center gap-2">
                            <ItemIcon path={s.item.iconPath} className="size-4" />
                            <span className="max-w-60 truncate">{s.item.name}</span>
                            <QualityTag quality={s.hq ? "HQ" : "NQ"} />
                          </span>
                        </td>
                        <td className="px-2 text-right tabular-nums">
                          {gil(s.quantity)} × {gil(s.unitPrice)}
                        </td>
                        <td className="px-2 text-right tabular-nums">{gil(net(s))}</td>
                        <td className="px-2 text-right text-muted-foreground tabular-nums">
                          {s.costUnits === 0
                            ? "—"
                            : `${gil(s.cost)}${s.costUnits < s.quantity ? ` (${String(s.costUnits)} u.)` : ""}`}
                        </td>
                        <td
                          className={cn(
                            "px-2 text-right font-medium tabular-nums",
                            p !== null && (p >= 0 ? "text-emerald-500" : "text-red-500"),
                          )}
                        >
                          {p === null ? "—" : `${p >= 0 ? "+" : ""}${gil(p)}`}
                        </td>
                        <td className="px-2 text-muted-foreground">
                          {s.source === "auto" ? "détectée" : "manuelle"}
                        </td>
                        <td className="w-8 pr-2">
                          <DeleteButton
                            label="Supprimer cette vente"
                            onClick={() => {
                              delSale.mutate(s.id, { onError });
                            }}
                          />
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </section>

          <section className="flex flex-col gap-2">
            <h2 className="text-sm font-semibold">Achats ({purchases.length})</h2>
            <div className="rounded-md border">
              <table className="w-full">
                <thead className="text-[11px] text-muted-foreground">
                  <tr>
                    <th className="px-3 py-1.5 text-left font-normal">Date</th>
                    <th className="px-2 text-left font-normal">Objet</th>
                    <th className="px-2 text-right font-normal">Quantité × prix payé</th>
                    <th className="px-2 text-right font-normal">Total</th>
                    <th className="px-2 text-right font-normal">Reste en stock</th>
                    <th className="px-2 text-left font-normal">Acheté sur</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {purchases.map((p) => (
                    <tr key={p.id} className="border-t border-border/50">
                      <td className="h-8 px-3 text-muted-foreground tabular-nums">
                        {date(p.boughtAt)}
                      </td>
                      <td className="px-2">
                        <span className="flex items-center gap-2">
                          <ItemIcon path={p.item.iconPath} className="size-4" />
                          <span className="max-w-60 truncate">{p.item.name}</span>
                          <QualityTag quality={p.hq ? "HQ" : "NQ"} />
                        </span>
                      </td>
                      <td className="px-2 text-right tabular-nums">
                        {gil(p.quantity)} × {gil(p.unitCost)}
                      </td>
                      <td className="px-2 text-right tabular-nums">
                        {gil(p.unitCost * p.quantity)}
                      </td>
                      <td className="px-2 text-right tabular-nums">{gil(p.remaining)}</td>
                      <td className="px-2 text-muted-foreground">{p.sourceWorld ?? "—"}</td>
                      <td className="w-8 pr-2">
                        <DeleteButton
                          label="Supprimer cet achat"
                          onClick={() => {
                            delPurchase.mutate(p.id, { onError });
                          }}
                        />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}
