import { useState, type ReactNode } from "react";
import { DatabaseIcon } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { errorMessage } from "@/lib/api";
import { ago } from "@/lib/format";
import { BuildScreen } from "@/spender/BuildScreen";
import { useBuildDataset, useStatus } from "@/spender/queries";

/** Les données du jeu changent à chaque patch : on propose de les relire au-delà. */
const DATA_MAX_AGE_MS = 14 * 24 * 3_600_000;

/**
 * Les outils de dépense ont besoin des données du jeu (marchands, échanges…),
 * récupérées une fois puis gardées sur le disque.
 */
export function SpenderGate({ children }: { children: ReactNode }) {
  const status = useStatus();
  const rebuild = useBuildDataset();
  const [now] = useState(() => Date.now());

  if (status.isPending) return <Skeleton className="m-4 h-7 w-64" />;
  if (status.data?.ready !== true) return <BuildScreen />;

  const builtAt = status.data.builtAtMs;
  const old = builtAt !== null && now - builtAt > DATA_MAX_AGE_MS;
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex min-h-0 flex-1 flex-col">{children}</div>
      <div className="flex h-7 shrink-0 items-center gap-2 border-t px-3 text-[11px] text-muted-foreground">
        <DatabaseIcon className="size-3" />
        Données du jeu {ago(builtAt)}
        {old && <span className="text-amber-500">· peut-être dépassées après un patch</span>}
        <Button
          variant="ghost"
          size="xs"
          className="ml-auto"
          disabled={rebuild.isPending}
          onClick={() => {
            rebuild.mutate(undefined, {
              onSuccess: () => toast.success("Données du jeu mises à jour."),
              onError: (e) => toast.error(errorMessage(e)),
            });
          }}
        >
          {rebuild.isPending ? "Mise à jour…" : "Mettre à jour les données du jeu"}
        </Button>
      </div>
    </div>
  );
}
