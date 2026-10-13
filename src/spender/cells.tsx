import { LockIcon } from "lucide-react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { ItemIcon } from "@/components/market";
import type { Unlock } from "@/spender/api";
import { ago, gil } from "@/lib/format";

export function ItemCell(props: { name: string; icon: string | null; hq?: boolean; qty?: number }) {
  return (
    <span className="flex items-center gap-2">
      <ItemIcon path={props.icon} className="size-5" />
      <span className="max-w-72 truncate">{props.name}</span>
      {props.hq && <span className="text-[10px] font-semibold text-amber-500">HQ</span>}
      {props.qty !== undefined && props.qty > 1 && (
        <span className="text-muted-foreground">×{props.qty}</span>
      )}
    </span>
  );
}

export function PriceCell(props: {
  value: number | null;
  minListing: number | null;
  averageSale: number | null;
  lastSaleMs?: number | null;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span>{gil(props.value)}</span>
      </TooltipTrigger>
      <TooltipContent>
        Annonce la moins chère : {gil(props.minListing)} · Vente moyenne : {gil(props.averageSale)}
        {props.lastSaleMs != null && ` · Dernière vente ${ago(props.lastSaleMs)}`}
      </TooltipContent>
    </Tooltip>
  );
}

export function VendorCell({ vendors, shops }: { vendors: string[]; shops?: string[] }) {
  const first = vendors[0];
  if (first === undefined) return <span className="text-muted-foreground">—</span>;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="cursor-help">
          {first}
          {vendors.length > 1 && (
            <span className="text-muted-foreground"> +{vendors.length - 1}</span>
          )}
        </span>
      </TooltipTrigger>
      <TooltipContent className="max-w-80">
        <div className="flex flex-col gap-1">
          <span>{vendors.join(", ")}</span>
          {shops && shops.length > 0 && <span className="opacity-70">{shops.join(" · ")}</span>}
        </div>
      </TooltipContent>
    </Tooltip>
  );
}

export function UnlockCell({ unlock }: { unlock: Unlock | null }) {
  if (!unlock) return <span className="text-muted-foreground">—</span>;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="inline-flex cursor-help items-center gap-1 rounded-sm bg-amber-500/15 px-1.5 py-0.5 text-[11px] text-amber-600 dark:text-amber-400">
          <LockIcon className="size-3" />
          Requis
        </span>
      </TooltipTrigger>
      <TooltipContent className="max-w-72">
        {unlock.reason ?? "Une condition est nécessaire pour accéder à cet échange."}
      </TooltipContent>
    </Tooltip>
  );
}

/** Montant coloré : vert si positif, rouge si négatif. */
export function SignedGil({ value }: { value: number | null }) {
  const color =
    value == null ? "" : value > 0 ? "text-emerald-600 dark:text-emerald-400" : "text-red-500";
  return <span className={`font-medium ${color}`}>{gil(value)}</span>;
}
