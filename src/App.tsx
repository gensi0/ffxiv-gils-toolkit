import { useEffect } from "react";
import { Sidebar } from "@/components/Sidebar";
import { StatusBar } from "@/components/StatusBar";
import { Skeleton } from "@/components/ui/skeleton";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useBackendEvents, useSettings } from "@/lib/queries";
import { type Page, useNav } from "@/store/nav";
import { DashboardPage } from "@/pages/DashboardPage";
import { ItemPage } from "@/pages/ItemPage";
import { WatchlistPage } from "@/pages/WatchlistPage";
import { ScanPage } from "@/pages/ScanPage";
import { SearchPage } from "@/pages/SearchPage";
import { ShoppingListPage } from "@/pages/ShoppingListPage";
import { JournalPage } from "@/pages/JournalPage";
import { VentesPage } from "@/pages/VentesPage";
import { SettingsPage } from "@/pages/SettingsPage";
import { StatsPage } from "@/pages/StatsPage";
import {
  DeliveriesRoute,
  DesynthRoute,
  GilShopRoute,
  MonnaiesRoute,
  VenturesRoute,
} from "@/spender/pages";

const PAGES: Record<Page, React.ComponentType> = {
  dashboard: DashboardPage,
  scan: ScanPage,
  watchlist: WatchlistPage,
  search: SearchPage,
  courses: ShoppingListPage,
  ventes: VentesPage,
  journal: JournalPage,
  stats: StatsPage,
  settings: SettingsPage,
  item: ItemPage,
  monnaies: MonnaiesRoute,
  pnj: GilShopRoute,
  livraisons: DeliveriesRoute,
  servants: VenturesRoute,
  desynthese: DesynthRoute,
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
