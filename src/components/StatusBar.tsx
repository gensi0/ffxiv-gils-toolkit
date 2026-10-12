import { useSettings, useWorlds } from "@/lib/queries";

export function StatusBar() {
  const { data: settings } = useSettings();
  const { data: worlds } = useWorlds();
  const home = worlds?.find((w) => w.id === settings?.homeWorldId);

  return (
    <footer className="flex h-6 shrink-0 items-center gap-4 border-t px-3 text-[11px] text-muted-foreground">
      <span>
        {home ? `${home.name} · ${home.dc}` : "No home world"}
        {settings && ` · retainers in ${settings.retainerCity}`}
      </span>
      {/* Rate-limit, scan progress and WebSocket state land here (M2, M3, M5). */}
      <span className="ml-auto">Universalis: idle</span>
    </footer>
  );
}
