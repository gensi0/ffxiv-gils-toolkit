import { useEffect, useState } from "react";
import { listen } from "@/lib/backend";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { errorMessage } from "@/lib/api";
import { EVENTS, type BuildProgress } from "@/spender/api";
import { useBuildDataset } from "@/spender/queries";

/** Premier lancement : récupère les données du jeu (≈ 15 à 30 s). */
export function BuildScreen() {
  const build = useBuildDataset();
  const [progress, setProgress] = useState<BuildProgress | null>(null);

  useEffect(() => {
    const unlisten = listen<BuildProgress>(EVENTS.buildProgress, (e) => {
      setProgress(e.payload);
    });
    return () => {
      void unlisten.then((f) => {
        f();
      });
    };
  }, []);

  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <div className="flex w-96 flex-col gap-4 text-sm">
        <h1 className="text-base font-semibold">Préparation des données du jeu</h1>
        <p className="text-xs text-muted-foreground">
          L'application récupère une fois la liste des marchands, de leurs échanges et des objets
          revendables. Les données sont ensuite gardées sur le serveur, pour tous les comptes.
        </p>
        {build.isPending ? (
          <div className="flex flex-col gap-2">
            <Progress value={progress ? (progress.done / progress.total) * 100 : 2} />
            <span className="text-xs text-muted-foreground">
              {progress ? `${progress.step}…` : "Démarrage…"}
            </span>
          </div>
        ) : (
          <>
            {build.isError && (
              <p className="text-xs text-destructive">{errorMessage(build.error)}</p>
            )}
            <Button
              onClick={() => {
                build.mutate();
              }}
            >
              {build.isError ? "Réessayer" : "Récupérer les données"}
            </Button>
          </>
        )}
      </div>
    </div>
  );
}
