// Small presentational pieces shared by the market views.
import { convertFileSrc } from "@tauri-apps/api/core";
import type { Confidence } from "@/lib/api";
import { ago, isStale } from "@/lib/format";
import { cn } from "@/lib/utils";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

/** Icons go through the backend `icon://` protocol (disk-cached XIVAPI assets). */
export function ItemIcon({ path, className }: { path: string | null; className?: string }) {
  const size = cn("size-6 shrink-0 rounded-sm", className);
  if (!path) return <div className={cn(size, "bg-muted")} />;
  return (
    <img
      src={convertFileSrc(path, "icon")}
      alt=""
      className={size}
      loading="lazy"
      draggable={false}
    />
  );
}

const CONFIDENCE: Record<Confidence, { label: string; className: string; hint: string }> = {
  HOT: {
    label: "Hot",
    className: "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400",
    hint: "Sells ≥ 5 units/day with under a day of competing supply.",
  },
  OK: {
    label: "OK",
    className: "bg-sky-500/15 text-sky-600 dark:text-sky-400",
    hint: "Steady demand, little competing supply.",
  },
  COMPETITIVE: {
    label: "Competitive",
    className: "bg-amber-500/15 text-amber-600 dark:text-amber-400",
    hint: "3–7 days of competing supply listed near your price.",
  },
  SATURATED: {
    label: "Saturated",
    className: "bg-red-500/15 text-red-600 dark:text-red-400",
    hint: "More than 7 days of competing supply listed near your price.",
  },
  SLOW: {
    label: "Slow",
    className: "bg-muted text-muted-foreground",
    hint: "Under 0.5 units sold per day: stock may rot.",
  },
};

export function ConfidenceBadge({ value }: { value: Confidence }) {
  const c = CONFIDENCE[value];
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          className={cn(
            "inline-flex h-5 items-center rounded-sm px-1.5 text-[11px] font-medium",
            c.className,
          )}
        >
          {c.label}
        </span>
      </TooltipTrigger>
      <TooltipContent>{c.hint}</TooltipContent>
    </Tooltip>
  );
}

export function QualityTag({ quality, soldAs }: { quality: "NQ" | "HQ"; soldAs?: "NQ" | "HQ" }) {
  return (
    <span className="text-[11px] text-muted-foreground">
      {quality}
      {soldAs && soldAs !== quality && (
        <span title="HQ bought, sold into NQ demand"> → {soldAs}</span>
      )}
    </span>
  );
}

/** Relative age, dimmed when stale. */
export function Age({ ms, staleHours }: { ms: number | null | undefined; staleHours?: number }) {
  const stale = isStale(ms, staleHours);
  return (
    <span
      className={cn("whitespace-nowrap", stale && "text-muted-foreground/60")}
      title={ms ? new Date(ms).toLocaleString() : undefined}
    >
      {ago(ms)}
    </span>
  );
}
