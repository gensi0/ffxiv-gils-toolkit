import { useNow } from "@/lib/hooks";
import { useSettings, useWorlds, isRunning } from "@/lib/queries";
import { PHASE_LABEL } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useLive } from "@/store/live";

export function StatusBar() {
  const { data: settings } = useSettings();
  const { data: worlds } = useWorlds();
  const net = useLive((s) => s.net);
  const scan = useLive((s) => s.scan);
  const ws = useLive((s) => s.ws);
  const home = worlds?.find((w) => w.id === settings?.homeWorldId);

  const backoffUntil = net?.backoffUntilMs ?? null;
  const now = useNow(backoffUntil !== null || ws?.state === "disconnected");
  const backoffSecs = backoffUntil !== null ? Math.ceil((backoffUntil - now) / 1000) : 0;
  const wsLabel =
    ws?.state === "connected"
      ? "Direct : connecté"
      : ws?.state === "connecting"
        ? "Direct : connexion…"
        : ws?.state === "disconnected"
          ? `Direct : reconnexion dans ${Math.max(0, Math.ceil((ws.retryAtMs - now) / 1000))} s`
          : null;
  const netLabel =
    backoffSecs > 0
      ? `Universalis : limite de requêtes, nouvel essai dans ${backoffSecs} s`
      : net && net.inFlight > 0
        ? `Universalis : ${net.inFlight} en cours`
        : "Universalis : inactif";

  return (
    <footer className="flex h-6 shrink-0 items-center gap-4 border-t px-3 text-[11px] text-muted-foreground">
      <span>
        {home ? `${home.name} · ${home.dc}` : "Aucun serveur principal"}
        {settings && ` · servants à ${settings.retainerCity}`}
      </span>
      {isRunning(scan) && scan && (
        <span className="text-foreground">
          {PHASE_LABEL[scan.phase]}{" "}
          {scan.total > 0 && `${Math.round((scan.done / scan.total) * 100)} %`}
        </span>
      )}
      <span className={cn("ml-auto", backoffSecs > 0 && "text-amber-600 dark:text-amber-400")}>
        {netLabel}
      </span>
      {wsLabel && (
        <span
          className={cn(ws?.state === "disconnected" && "text-amber-600 dark:text-amber-400")}
          title={ws?.state === "disconnected" ? ws.error : undefined}
        >
          {wsLabel}
        </span>
      )}
    </footer>
  );
}
