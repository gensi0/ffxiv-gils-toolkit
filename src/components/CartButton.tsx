import { ShoppingCartIcon } from "lucide-react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { Opportunity } from "@/lib/api";
import { cn } from "@/lib/utils";
import { shoppingKey, useShoppingList } from "@/store/shoppingList";

/** Ajoute ou retire une opportunité de la liste de courses. */
export function CartButton({ o }: { o: Opportunity }) {
  const key = shoppingKey(o);
  const inList = useShoppingList((s) => s.items.some((i) => i.key === key));
  const toggle = useShoppingList((s) => s.toggle);
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={inList ? "Retirer de la liste de courses" : "Ajouter à la liste de courses"}
          onClick={(e) => {
            // La ligne ouvre la fiche de l'objet : on ne veut pas ça ici.
            e.stopPropagation();
            toggle(o);
          }}
          className={cn(
            "inline-flex size-6 items-center justify-center rounded-md hover:bg-muted",
            inList ? "text-emerald-500" : "text-muted-foreground/60 hover:text-foreground",
          )}
        >
          <ShoppingCartIcon className={cn("size-3.5", inList && "fill-emerald-500/30")} />
        </button>
      </TooltipTrigger>
      <TooltipContent>
        {inList
          ? "Dans ta liste de courses (cliquer pour retirer)"
          : "Ajouter à la liste de courses"}
      </TooltipContent>
    </Tooltip>
  );
}
