import { useMemo } from "react";
import { ItemIcon } from "@/components/market";
import { CATEGORY_LABEL, type Category, type Currency } from "@/spender/api";
import { cn } from "@/lib/utils";
import { usePluginStatus } from "@/lib/queries";

const ORDER: Category[] = [
  "assignats",
  "memoquartz",
  "grandeCompagnie",
  "tribusAlliees",
  "jcj",
  "chasse",
  "embranchement",
  "autres",
];

export function CurrencySidebar(props: {
  currencies: Currency[];
  selected: number | null;
  onSelect: (id: number) => void;
}) {
  // Quantités possédées, si le plugin Dalamud est installé.
  const owned = usePluginStatus().data?.owned;
  const groups = useMemo(
    () =>
      ORDER.map((cat) => ({
        cat,
        items: props.currencies.filter((c) => c.category === cat),
      })).filter((g) => g.items.length > 0),
    [props.currencies],
  );

  return (
    <nav className="flex w-56 shrink-0 flex-col overflow-y-auto border-r bg-sidebar p-2">
      {groups.map((g) => (
        <section key={g.cat} className="mb-2">
          <h2 className="px-2 py-1 text-[11px] font-medium text-muted-foreground uppercase">
            {CATEGORY_LABEL[g.cat]}
          </h2>
          {g.items.map((c) => (
            <button
              key={c.itemId}
              type="button"
              onClick={() => {
                props.onSelect(c.itemId);
              }}
              className={cn(
                "flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-xs text-muted-foreground hover:bg-muted hover:text-foreground",
                props.selected === c.itemId && "bg-muted text-foreground",
              )}
            >
              <ItemIcon path={c.icon} className="size-4" />
              <span className="truncate">{c.name}</span>
              {owned ? (
                <span
                  className={cn(
                    "ml-auto text-[10px] tabular-nums",
                    (owned[String(c.itemId)] ?? 0) > 0 ? "text-foreground" : "opacity-40",
                  )}
                  title="Quantité possédée (plugin)"
                >
                  {(owned[String(c.itemId)] ?? 0).toLocaleString("fr-FR")}
                </span>
              ) : (
                <span
                  className="ml-auto text-[10px] tabular-nums opacity-60"
                  title="Objets revendables"
                >
                  {c.itemCount}
                </span>
              )}
            </button>
          ))}
        </section>
      ))}
    </nav>
  );
}
