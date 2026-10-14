import { useMemo, useState } from "react";
import { ArrowDownIcon, ArrowUpIcon, Loader2Icon, SparklesIcon, StarOffIcon } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { DataTable } from "@/components/DataTable";
import { EmptyState, PageHeader } from "@/components/Page";
import { Age, ConfidenceBadge, ItemIcon, QualityTag } from "@/components/market";
import { errorMessage, type WatchChange, type WatchItem } from "@/lib/api";
import { gil } from "@/lib/format";
import { useNow } from "@/lib/hooks";
import { useSetWatchThreshold, useSettings, useToggleWatch, useWatchlist } from "@/lib/queries";
import { columnHelper } from "@/lib/table";
import { useNav } from "@/store/nav";

/** A change stays highlighted this long. */
const HIGHLIGHT_MS = 10 * 60 * 1000;

const CHANGE_STYLE: Record<WatchChange, { row: string; icon: React.ReactNode; label: string }> = {
  new: {
    row: "bg-emerald-500/10",
    icon: <SparklesIcon className="size-3 text-emerald-500" />,
    label: "Nouvelle opportunité",
  },
  up: {
    row: "bg-emerald-500/10",
    icon: <ArrowUpIcon className="size-3 text-emerald-500" />,
    label: "Bénéfice en hausse",
  },
  down: {
    row: "bg-red-500/10",
    icon: <ArrowDownIcon className="size-3 text-red-500" />,
    label: "Bénéfice en baisse",
  },
};

const isRecent = (w: WatchItem, now: number) =>
  w.change !== null && w.changedAt !== null && now - w.changedAt < HIGHLIGHT_MS;

/** Per-item notification threshold, saved on blur / Enter. Empty = default. */
function ThresholdInput({ row, placeholder }: { row: WatchItem; placeholder: number }) {
  const save = useSetWatchThreshold();
  const [text, setText] = useState(row.notifyAboveProfit?.toString() ?? "");
  const commit = () => {
    const clean = text.replace(/[\s,_]/g, "");
    const value = clean === "" ? null : Number(clean);
    if (value !== null && (!Number.isInteger(value) || value < 0)) {
      toast.error("Le seuil doit être un nombre entier de gils");
      return;
    }
    if (value === row.notifyAboveProfit) return;
    save.mutate(
      { itemId: row.itemId, profit: value },
      { onError: (e) => toast.error(errorMessage(e)) },
    );
  };
  return (
    <Input
      inputMode="numeric"
      value={text}
      placeholder={gil(placeholder)}
      onClick={(e) => {
        e.stopPropagation();
      }}
      onChange={(e) => {
        setText(e.target.value);
      }}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
      }}
      className="h-6 w-28 text-right"
    />
  );
}

const c = columnHelper<WatchItem>();

export function WatchlistPage() {
  const list = useWatchlist();
  const settings = useSettings();
  const toggle = useToggleWatch();
  const openItem = useNav((s) => s.openItem);
  const setPage = useNav((s) => s.setPage);
  const now = useNow(true, 30_000);
  const defaultThreshold = settings.data?.watchNotifyProfit ?? 0;

  const columns = useMemo(
    () => [
      c.accessor((w) => w.item?.name ?? `Objet n°${w.itemId}`, {
        id: "item",
        header: "Objet",
        cell: (v) => {
          const w = v.row.original;
          const style = w.change ? CHANGE_STYLE[w.change] : null;
          return (
            <span className="flex items-center gap-2">
              <ItemIcon path={w.item?.iconPath ?? null} className="size-5" />
              <span className="max-w-64 truncate">{v.getValue()}</span>
              {style && isRecent(w, now) && <span title={style.label}>{style.icon}</span>}
              {!w.loaded && <Loader2Icon className="size-3 animate-spin text-muted-foreground" />}
              {w.error && (
                <span className="text-[11px] text-destructive" title={w.error}>
                  données partielles
                </span>
              )}
            </span>
          );
        },
      }),
      c.accessor((w) => w.best?.world, {
        id: "source",
        header: "Meilleure source",
        cell: (v) => {
          const b = v.row.original.best;
          return b ? (
            <span>
              {b.world} <span className="text-muted-foreground">· {b.dc}</span>{" "}
              <QualityTag quality={b.quality} soldAs={b.soldAs} />
            </span>
          ) : (
            <span className="text-muted-foreground">aucune</span>
          );
        },
        sortUndefined: "last",
      }),
      c.accessor((w) => w.best?.sourceUnitPrice, {
        id: "buy",
        header: "Achat à",
        cell: (v) => gil(v.getValue()),
        meta: { numeric: true },
        sortUndefined: "last",
      }),
      c.accessor((w) => w.best?.unitsToBuy, {
        id: "units",
        header: "Unités",
        cell: (v) => gil(v.getValue()),
        meta: { numeric: true },
        sortUndefined: "last",
      }),
      c.accessor((w) => w.best?.expectedSell, {
        id: "sell",
        header: "Vente à",
        cell: (v) => gil(v.getValue()),
        meta: { numeric: true },
        sortUndefined: "last",
      }),
      c.accessor((w) => w.homeMinNq ?? undefined, {
        id: "homeMin",
        header: "Min NQ principal",
        cell: (v) => gil(v.getValue()),
        meta: { numeric: true },
        sortUndefined: "last",
      }),
      c.accessor((w) => w.best?.profitPerTrip, {
        id: "trip",
        header: "Bénéfice / trajet",
        cell: (v) => gil(v.getValue()),
        meta: { numeric: true },
        sortUndefined: "last",
      }),
      c.accessor((w) => w.best?.dailyProfit, {
        id: "daily",
        header: "Gils / jour",
        cell: (v) => <span className="font-medium">{gil(v.getValue())}</span>,
        meta: { numeric: true },
        sortUndefined: "last",
      }),
      c.accessor((w) => w.best?.confidence, {
        id: "confidence",
        header: "Confiance",
        cell: (v) => {
          const value = v.getValue();
          return value ? <ConfidenceBadge value={value} /> : null;
        },
        sortUndefined: "last",
      }),
      c.accessor((w) => w.lastEventMs ?? undefined, {
        id: "event",
        header: "Dernière MàJ en direct",
        cell: (v) => <Age ms={v.getValue()} staleHours={1} />,
        meta: { numeric: true },
        sortUndefined: "last",
      }),
      c.display({
        id: "threshold",
        header: "Notifier au-delà de",
        cell: (v) => <ThresholdInput row={v.row.original} placeholder={defaultThreshold} />,
        meta: { numeric: true },
      }),
      c.display({
        id: "remove",
        header: "",
        cell: (v) => (
          <Button
            variant="ghost"
            size="icon-xs"
            title="Retirer du suivi"
            onClick={(e) => {
              e.stopPropagation();
              toggle.mutate({ itemId: v.row.original.itemId, watched: true });
            }}
          >
            <StarOffIcon />
          </Button>
        ),
      }),
    ],
    [now, defaultThreshold, toggle],
  );

  return (
    <>
      <PageHeader title="Surveillance du marché" />
      {list.isError ? (
        <div className="flex items-center gap-3 p-4 text-xs text-destructive">
          {errorMessage(list.error)}
          <Button variant="outline" size="sm" onClick={() => void list.refetch()}>
            Réessayer
          </Button>
        </div>
      ) : !list.data ? (
        <div className="flex flex-col gap-1 p-4">
          {Array.from({ length: 5 }, (_, i) => (
            <Skeleton key={i} className="h-8" />
          ))}
        </div>
      ) : list.data.length === 0 ? (
        <EmptyState
          message="Aucun objet surveillé. Ajoutes-en un depuis sa fiche (« Surveiller le marché ») pour suivre ses prix en direct."
          action={
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                setPage("search");
              }}
            >
              Rechercher des objets
            </Button>
          }
        />
      ) : (
        <DataTable
          columns={columns}
          data={list.data}
          initialSorting={[{ id: "daily", desc: true }]}
          getRowId={(w) => String(w.itemId)}
          onRowClick={(w) => {
            openItem(w.itemId);
          }}
          rowClassName={(w) =>
            w.change && isRecent(w, now) ? CHANGE_STYLE[w.change].row : undefined
          }
        />
      )}
    </>
  );
}
