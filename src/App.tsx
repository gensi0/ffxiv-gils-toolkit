import { useEffect } from "react";
import { Sidebar } from "@/components/Sidebar";
import { StatusBar } from "@/components/StatusBar";
import { Skeleton } from "@/components/ui/skeleton";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useBackendEvents, useSettings } from "@/lib/queries";
import { type Page, useNav } from "@/store/nav";
import { DashboardPage } from "@/pages/DashboardPage";
import { HistoryPage } from "@/pages/HistoryPage";
import { ItemPage } from "@/pages/ItemPage";
import { WatchlistPage } from "@/pages/Placeholders";
import { ScanPage } from "@/pages/ScanPage";
import { SearchPage } from "@/pages/SearchPage";
import { SettingsPage } from "@/pages/SettingsPage";

const PAGES: Record<Page, React.ComponentType> = {
  dashboard: DashboardPage,
  scan: ScanPage,
  watchlist: WatchlistPage,
  search: SearchPage,
  history: HistoryPage,
  settings: SettingsPage,
  item: ItemPage,
};

export default function App() {
  const settings = useSettings();
  const page = useNav((s) => s.page);
  const theme = settings.data?.theme ?? "dark";
  useBackendEvents();

  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
  }, [theme]);

  // Until a home world is chosen, the setup form is the whole app.
  const needsSetup = settings.data?.homeWorldId === null;
  const Current = PAGES[page];

  return (
    <TooltipProvider>
      <div className="flex h-full flex-col">
        <div className="flex min-h-0 flex-1">
          {!needsSetup && <Sidebar />}
          <main className="flex min-w-0 flex-1 flex-col">
            {settings.isPending ? (
              <Skeleton className="m-4 h-7 w-64" />
            ) : needsSetup ? (
              <SettingsPage firstLaunch />
            ) : (
              <Current />
            )}
          </main>
        </div>
        <StatusBar />
      </div>
      <Toaster theme={theme} position="bottom-right" />
    </TooltipProvider>
  );
}
