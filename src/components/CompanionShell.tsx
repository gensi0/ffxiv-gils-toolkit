import type { ReactNode } from "react";
import { useShoppingList } from "@/store/shoppingList";
import { useVentes } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { type Page, useNav } from "@/store/nav";
import { COMPANION_PAGES } from "@/lib/companionPages";

/** Mise en page téléphone : contenu plein écran et barre d'onglets en bas. */
export function CompanionShell({ children }: { children: ReactNode }) {
  const page = useNav((s) => s.page);
  const setPage = useNav((s) => s.setPage);
  const toBuy = useShoppingList((s) => s.items.filter((i) => !i.bought).length);
  const ventes = useVentes();
  const alerts =
    (ventes.data?.pending.length ?? 0) +
    (ventes.data?.listings.filter((l) => l.rank !== null && l.rank > ventes.data.topRank).length ??
      0);
  const badge: Partial<Record<Page, number>> = { ventes: alerts, courses: toBuy };

  return (
    <div className="flex h-full flex-col">
      <main className="flex min-h-0 flex-1 flex-col overflow-y-auto">{children}</main>
      <nav className="flex shrink-0 border-t bg-background pb-[env(safe-area-inset-bottom)]">
        {COMPANION_PAGES.map(({ page: p, label, icon: Icon }) => (
          <button
            key={p}
            type="button"
            className={cn(
              "relative flex flex-1 flex-col items-center gap-0.5 py-2 text-[10px]",
              page === p ? "text-foreground" : "text-muted-foreground",
            )}
            onClick={() => {
              setPage(p);
            }}
          >
            <Icon className="size-5" />
            {label}
            {(badge[p] ?? 0) > 0 && (
              <span className="absolute top-1 right-[calc(50%-18px)] rounded-full bg-red-500 px-1 text-[9px] text-white tabular-nums">
                {badge[p]}
              </span>
            )}
          </button>
        ))}
      </nav>
    </div>
  );
}
