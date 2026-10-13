import { TagIcon } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { api, errorMessage } from "@/lib/api";
import { useVentes, useVentesAction } from "@/lib/queries";
import { cn } from "@/lib/utils";

/** Suit mes annonces de cet objet dans « Mes ventes », même sans achat noté. */
export function TrackSalesButton({ itemId }: { itemId: number }) {
  const ventes = useVentes();
  const tracked = ventes.data?.tracked.some((i) => i.id === itemId) ?? false;
  const toggle = useVentesAction((on: boolean) => api.ventesTrack(itemId, on));
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          disabled={toggle.isPending || ventes.isPending}
          onClick={() => {
            toggle.mutate(!tracked, { onError: (e) => toast.error(errorMessage(e)) });
          }}
        >
          <TagIcon className={cn(tracked && "fill-emerald-500/30 text-emerald-500")} />
          {tracked ? "Ventes suivies" : "Suivre mes ventes"}
        </Button>
      </TooltipTrigger>
      <TooltipContent className="max-w-64">
        Surveille tes annonces de cet objet sur ton serveur : alerte si on passe devant toi,
        détection des ventes dans le journal.
      </TooltipContent>
    </Tooltip>
  );
}
