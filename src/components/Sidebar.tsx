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
  TagIcon,
  NotebookPenIcon,
  StarIcon,
  StoreIcon,
  UsersIcon,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { type Page, useNav } from "@/store/nav";
import { useShoppingList } from "@/store/shoppingList";
import { useVentes } from "@/lib/queries";

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
      { page: "ventes", label: "Mes ventes", icon: TagIcon },
      { page: "journal", label: "Journal des gains", icon: NotebookPenIcon },
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
  // Annonces devancées ou à confirmer, en pastille sur « Mes ventes ».
  const ventes = useVentes();
  const alerts =
    (ventes.data?.pending.length ?? 0) +
    (ventes.data?.listings.filter((l) => l.rank !== null && l.rank > ventes.data.topRank).length ??
      0);
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
      {page === "ventes" && alerts > 0 && (
        <span className="ml-auto rounded-full bg-red-500/20 px-1.5 text-[10px] text-red-500 tabular-nums">
          {alerts}
        </span>
      )}
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
