import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { gil } from "@/lib/format";
import { useVentes } from "@/lib/queries";

/** « Déjà à toi » : unités en vente par tes servants et en stock (achats notés). */
export function MyStockBadge({ itemId }: { itemId: number }) {
  const ventes = useVentes();
  const d = ventes.data;
  if (!d) return null;
  const listed = d.listings.filter((l) => l.item.id === itemId).reduce((t, l) => t + l.quantity, 0);
  const stock = d.stock.filter((s) => s.item.id === itemId).reduce((t, s) => t + s.remaining, 0);
  if (listed === 0 && stock === 0) return null;
  const parts = [
    listed > 0 ? `${gil(listed)} en vente` : null,
    stock > 0 ? `${gil(stock)} en stock` : null,
  ].filter(Boolean);
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="shrink-0 rounded bg-sky-500/15 px-1 text-[10px] text-sky-500">
          {parts.join(" · ")}
        </span>
      </TooltipTrigger>
      <TooltipContent className="max-w-64">
        Tu en as déjà : vérifie avant d'en racheter. Tes propres annonces ne comptent pas comme
        concurrence dans le calcul de revente.
      </TooltipContent>
    </Tooltip>
  );
}
