import {
  BarChart2Icon,
  CoinsIcon,
  HammerIcon,
  LayoutDashboardIcon,
  type LucideIcon,
  PackageCheckIcon,
  RadarIcon,
  SearchIcon,
  SettingsIcon,
  ShoppingCartIcon,
  StarIcon,
  StoreIcon,
  UsersIcon,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { type Page, useNav } from "@/store/nav";
import { useShoppingList } from "@/store/shoppingList";

interface Item {
  page: Page;
  label: string;
  icon: LucideIcon;
}

const SECTIONS: { title: string; items: Item[] }[] = [
  {
    title: "Marché",
    items: [
      { page: "dashboard", label: "Tableau de bord", icon: LayoutDashboardIcon },
      { page: "scan", label: "Scan", icon: RadarIcon },
      { page: "watchlist", label: "Suivi", icon: StarIcon },
      { page: "courses", label: "Liste de courses", icon: ShoppingCartIcon },
      { page: "search", label: "Recherche", icon: SearchIcon },
      { page: "stats", label: "Statistiques", icon: BarChart2Icon },
    ],
  },
  {
    title: "Dépenser",
    items: [
      { page: "monnaies", label: "Monnaies", icon: CoinsIcon },
      { page: "pnj", label: "Revente PNJ", icon: StoreIcon },
      { page: "livraisons", label: "Livraisons GC", icon: PackageCheckIcon },
      { page: "servants", label: "Servants", icon: UsersIcon },
      { page: "desynthese", label: "Désynthèse", icon: HammerIcon },
    ],
  },
];

function NavButton({ page, label, icon: Icon }: Item) {
  const current = useNav((s) => s.page);
  const setPage = useNav((s) => s.setPage);
  // Objets restant à acheter, affichés en pastille sur « Liste de courses ».
  const toBuy = useShoppingList((s) => s.items.filter((i) => !i.bought).length);
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
      {page === "courses" && toBuy > 0 && (
        <span className="ml-auto rounded-full bg-emerald-500/20 px-1.5 text-[10px] text-emerald-500 tabular-nums">
          {toBuy}
        </span>
      )}
    </button>
  );
}

export function Sidebar() {
  return (
    <nav className="flex w-44 shrink-0 flex-col gap-0.5 border-r bg-sidebar p-2">
      <div className="px-2 pt-1 pb-2 text-xs font-semibold tracking-tight">FFXIV Gils Toolkit</div>
      {SECTIONS.map((s) => (
        <section key={s.title} className="mb-2 flex flex-col gap-0.5">
          <h2 className="px-2 py-1 text-[10px] font-medium tracking-wide text-muted-foreground uppercase">
            {s.title}
          </h2>
          {s.items.map((item) => (
            <NavButton key={item.page} {...item} />
          ))}
        </section>
      ))}
      <div className="mt-auto">
        <NavButton page="settings" label="Paramètres" icon={SettingsIcon} />
      </div>
    </nav>
  );
}
