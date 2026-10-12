import { useEffect, useState } from "react";
import { useSettings, useWorlds, isRunning } from "@/lib/queries";
import { PHASE_LABEL } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useLive } from "@/store/live";

/** Current time, refreshed every second while `active` (for countdowns). */
function useNow(active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    const t = setInterval(() => {
      setNow(Date.now());
    }, 1000);
    return () => {
      clearInterval(t);
    };
  }, [active]);
  return now;
}

export function StatusBar() {
  const { data: settings } = useSettings();
  const { data: worlds } = useWorlds();
  const net = useLive((s) => s.net);
  const scan = useLive((s) => s.scan);
  const home = worlds?.find((w) => w.id === settings?.homeWorldId);

  const backoffUntil = net?.backoffUntilMs ?? null;
  const now = useNow(backoffUntil !== null);
  const backoffSecs = backoffUntil !== null ? Math.ceil((backoffUntil - now) / 1000) : 0;
  const netLabel =
    backoffSecs > 0
      ? `Universalis: rate limited, retrying in ${backoffSecs}s`
      : net && net.inFlight > 0
        ? `Universalis: ${net.inFlight} in flight`
        : "Universalis: idle";

  return (
    <footer className="flex h-6 shrink-0 items-center gap-4 border-t px-3 text-[11px] text-muted-foreground">
      <span>
        {home ? `${home.name} · ${home.dc}` : "No home world"}
        {settings && ` · retainers in ${settings.retainerCity}`}
      </span>
      {isRunning(scan) && scan && (
        <span className="text-foreground">
          {PHASE_LABEL[scan.phase]}{" "}
          {scan.total > 0 && `${Math.round((scan.done / scan.total) * 100)}%`}
        </span>
      )}
      <span className={cn("ml-auto", backoffSecs > 0 && "text-amber-600 dark:text-amber-400")}>
        {netLabel}
      </span>
      {/* WebSocket state lands here in M5. */}
    </footer>
  );
}
