import {
  HistoryIcon,
  LayoutDashboardIcon,
  type LucideIcon,
  RadarIcon,
  SearchIcon,
  SettingsIcon,
  StarIcon,
  BarChart2Icon,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { type Page, useNav } from "@/store/nav";

const ITEMS: { page: Page; label: string; icon: LucideIcon }[] = [
  { page: "dashboard", label: "Dashboard", icon: LayoutDashboardIcon },
  { page: "scan", label: "Scan", icon: RadarIcon },
  { page: "watchlist", label: "Watchlist", icon: StarIcon },
  { page: "search", label: "Search", icon: SearchIcon },
  { page: "history", label: "History", icon: HistoryIcon },
  { page: "stats", label: "Stats", icon: BarChart2Icon },
];

function NavButton({ page, label, icon: Icon }: { page: Page; label: string; icon: LucideIcon }) {
  const current = useNav((s) => s.page);
  const setPage = useNav((s) => s.setPage);
  return (
    <button
      type="button"
      onClick={() => {
        setPage(page);
      }}
      className={cn(
        "flex h-7 w-full items-center gap-2 rounded-md px-2 text-xs text-muted-foreground hover:bg-muted hover:text-foreground",
        current === page && "bg-muted text-foreground",
      )}
    >
      <Icon className="size-3.5" />
      {label}
    </button>
  );
}

export function Sidebar() {
  return (
    <nav className="flex w-44 shrink-0 flex-col gap-0.5 border-r bg-sidebar p-2">
      <div className="px-2 pt-1 pb-2 text-xs font-semibold tracking-tight">FFXIV Trader</div>
      {ITEMS.map((item) => (
        <NavButton key={item.page} {...item} />
      ))}
      <div className="mt-auto">
        <NavButton page="settings" label="Settings" icon={SettingsIcon} />
      </div>
    </nav>
  );
}
