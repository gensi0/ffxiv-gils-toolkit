import type { ReactNode } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { Lot } from "@/lib/api";
import { gil } from "@/lib/format";

/** Détail des annonces à acheter, pour une infobulle ou une ligne. */
export function LotList({ lots, max }: { lots: Lot[]; max: number | null }) {
  if (lots.length === 0) {
    return <span>Détail des annonces indisponible pour ce scan : relance un scan.</span>;
  }
  return (
    <div className="flex flex-col gap-0.5">
      <span className="font-medium">Annonces à acheter :</span>
      {lots.map((l, i) => (
        <span key={i} className="tabular-nums">
          {gil(l.quantity)} × {gil(l.price)} = {gil(l.quantity * l.price)}
        </span>
      ))}
      {max !== null && (
        <span className="mt-1 opacity-80">
          N'achète pas d'annonce au-dessus de {gil(max)} par unité. Les annonces moins chères non
          listées sont trop grosses pour ce que ton serveur écoule.
        </span>
      )}
    </div>
  );
}

export function LotsTooltip(props: { lots: Lot[]; max: number | null; children: ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="cursor-help underline decoration-dotted underline-offset-2">
          {props.children}
        </span>
      </TooltipTrigger>
      <TooltipContent className="max-w-72">
        <LotList lots={props.lots} max={props.max} />
      </TooltipContent>
    </Tooltip>
  );
}

/** Pastilles « 4 × 200 » pour la liste de courses. */
export function LotChips({ lots }: { lots: Lot[] }) {
  return (
    <span className="flex flex-wrap gap-1">
      {lots.map((l, i) => (
        <span
          key={i}
          className="rounded-sm bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground tabular-nums"
        >
          {gil(l.quantity)} × {gil(l.price)}
        </span>
      ))}
    </span>
  );
}
