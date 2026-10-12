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
    label: "New opportunity",
  },
  up: {
    row: "bg-emerald-500/10",
    icon: <ArrowUpIcon className="size-3 text-emerald-500" />,
    label: "Profit up",
  },
  down: {
    row: "bg-red-500/10",
    icon: <ArrowDownIcon className="size-3 text-red-500" />,
    label: "Profit down",
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
      toast.error("Threshold must be a whole number of gil");
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
      c.accessor((w) => w.item?.name ?? `Item #${w.itemId}`, {
        id: "item",
        header: "Item",
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
                  partial data
                </span>
              )}
            </span>
          );
        },
      }),
      c.accessor((w) => w.best?.world, {
        id: "source",
        header: "Best source",
        cell: (v) => {
          const b = v.row.original.best;
          return b ? (
            <span>
              {b.world} <span className="text-muted-foreground">· {b.dc}</span>{" "}
              <QualityTag quality={b.quality} soldAs={b.soldAs} />
            </span>
          ) : (
            <span className="text-muted-foreground">none</span>
          );
        },
        sortUndefined: "last",
      }),
      c.accessor((w) => w.best?.sourceUnitPrice, {
        id: "buy",
        header: "Buy at",
        cell: (v) => gil(v.getValue()),
        meta: { numeric: true },
        sortUndefined: "last",
      }),
      c.accessor((w) => w.best?.unitsToBuy, {
        id: "units",
        header: "Units",
        cell: (v) => gil(v.getValue()),
        meta: { numeric: true },
        sortUndefined: "last",
      }),
      c.accessor((w) => w.best?.expectedSell, {
        id: "sell",
        header: "Sell at",
        cell: (v) => gil(v.getValue()),
        meta: { numeric: true },
        sortUndefined: "last",
      }),
      c.accessor((w) => w.homeMinNq ?? undefined, {
        id: "homeMin",
        header: "Home min NQ",
        cell: (v) => gil(v.getValue()),
        meta: { numeric: true },
        sortUndefined: "last",
      }),
      c.accessor((w) => w.best?.profitPerTrip, {
        id: "trip",
        header: "Profit / trip",
        cell: (v) => gil(v.getValue()),
        meta: { numeric: true },
        sortUndefined: "last",
      }),
      c.accessor((w) => w.best?.dailyProfit, {
        id: "daily",
        header: "Gil / day",
        cell: (v) => <span className="font-medium">{gil(v.getValue())}</span>,
        meta: { numeric: true },
        sortUndefined: "last",
      }),
      c.accessor((w) => w.best?.confidence, {
        id: "confidence",
        header: "Confidence",
        cell: (v) => {
          const value = v.getValue();
          return value ? <ConfidenceBadge value={value} /> : null;
        },
        sortUndefined: "last",
      }),
      c.accessor((w) => w.lastEventMs ?? undefined, {
        id: "event",
        header: "Last live update",
        cell: (v) => <Age ms={v.getValue()} staleHours={1} />,
        meta: { numeric: true },
        sortUndefined: "last",
      }),
      c.display({
        id: "threshold",
        header: "Notify above",
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
            title="Remove from watchlist"
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
      <PageHeader title="Watchlist" />
      {list.isError ? (
        <div className="flex items-center gap-3 p-4 text-xs text-destructive">
          {errorMessage(list.error)}
          <Button variant="outline" size="sm" onClick={() => void list.refetch()}>
            Retry
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
          message="No starred items. Star one from its detail page to follow it live."
          action={
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                setPage("search");
              }}
            >
              Search items
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
