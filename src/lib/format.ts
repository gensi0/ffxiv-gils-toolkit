import type { ScanPhase } from "./api";

const gilFormat = new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 });
const decimalFormat = new Intl.NumberFormat("en-US", { maximumFractionDigits: 1 });

/** 1234567 → "1,234,567". */
export function gil(n: number | null | undefined): string {
  return n == null || !Number.isFinite(n) ? "—" : gilFormat.format(Math.round(n));
}

/** 1234567 → "1.2M", for dense cards. */
export function gilShort(n: number): string {
  const abs = Math.abs(n);
  if (abs >= 1e6) return `${decimalFormat.format(n / 1e6)}M`;
  if (abs >= 1e4) return `${decimalFormat.format(n / 1e3)}k`;
  return gil(n);
}

export function decimal(n: number | null | undefined): string {
  return n == null || !Number.isFinite(n) ? "—" : decimalFormat.format(n);
}

export function percent(ratio: number): string {
  return `${decimalFormat.format(ratio * 100)}%`;
}

/** Unix ms → "3h ago". */
export function ago(ms: number | null | undefined, now = Date.now()): string {
  if (ms == null) return "—";
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return "just now";
  const m = Math.round(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.round(m / 60);
  if (h < 48) return `${h}h ago`;
  return `${Math.round(h / 24)}d ago`;
}

/** Seconds → "2m 05s". */
export function duration(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = Math.round(secs % 60);
  return m > 0 ? `${m}m ${String(s).padStart(2, "0")}s` : `${s}s`;
}

/** Data older than this many hours is shown dimmed. */
export const STALE_HOURS = 6;

export function isStale(ms: number | null | undefined, hours = STALE_HOURS, now = Date.now()) {
  return ms == null || now - ms > hours * 3_600_000;
}

export const PHASE_LABEL: Record<ScanPhase, string> = {
  preparing: "Preparing",
  prefilter: "Scanning prices",
  drillDown: "Analyzing candidates",
  saving: "Saving",
  done: "Done",
  failed: "Failed",
  cancelled: "Cancelled",
};
