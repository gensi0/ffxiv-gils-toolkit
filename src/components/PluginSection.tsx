// Plugin Dalamud (client web) : installation, jetons, état de la synchronisation.
import { useEffect, useState } from "react";
import { CopyIcon, KeyRoundIcon, Trash2Icon } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/api";
import { serverCall, serverGet } from "@/lib/backend";
import { ago } from "@/lib/format";
import { usePluginStatus } from "@/lib/queries";

interface Token {
  id: number;
  label: string;
  createdAt: number;
  lastUsedAt: number | null;
}

function copy(text: string) {
  void navigator.clipboard.writeText(text).then(
    () => toast.success("Copié"),
    () => toast.error("Copie impossible : sélectionne le texte à la main."),
  );
}

export function PluginSection() {
  const repo = `${location.origin}/plugin/pluginmaster.json`;
  const [tokens, setTokens] = useState<Token[]>([]);
  const [fresh, setFresh] = useState<string | null>(null);
  const status = usePluginStatus();

  const reload = () => {
    serverGet<Token[]>("/api/plugin/tokens")
      .then(setTokens)
      .catch((e: unknown) => toast.error(errorMessage(e)));
  };
  useEffect(reload, []);

  const create = () => {
    serverCall<{ token: string }>("/api/plugin/tokens", { label: "Plugin Dalamud" })
      .then((t) => {
        setFresh(t.token);
        reload();
      })
      .catch((e: unknown) => toast.error(errorMessage(e)));
  };

  const revoke = (id: number) => {
    serverCall("/api/plugin/tokens/revoke", { id })
      .then(reload)
      .catch((e: unknown) => toast.error(errorMessage(e)));
  };

  const character = status.data?.character;

  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-[11px] font-semibold tracking-wider text-muted-foreground uppercase">
        Plugin Dalamud
      </h2>
      <p className="text-xs text-muted-foreground">
        Le plugin lit en jeu tes annonces (ouvre chaque servant à la sonnette), tes ventes, tes
        achats à l'HV, tes monnaies, ton inventaire et tes niveaux, et les envoie ici. Lecture seule
        : il n'automatise rien et n'envoie rien au jeu.
      </p>

      <ol className="flex list-decimal flex-col gap-2 rounded-md border p-3 pl-7 text-xs">
        <li>
          Dans le jeu : <span className="font-medium">/xlsettings</span> → Expérimental → Dépôts de
          plugins personnalisés, ajoute :
          <div className="mt-1 flex items-center gap-2">
            <code className="truncate rounded bg-muted px-1.5 py-0.5">{repo}</code>
            <Button
              type="button"
              size="icon"
              variant="ghost"
              className="size-6"
              onClick={() => {
                copy(repo);
              }}
            >
              <CopyIcon />
            </Button>
          </div>
        </li>
        <li>
          Installe <span className="font-medium">Gils Toolkit</span> depuis{" "}
          <span className="font-medium">/xlplugins</span>, puis ouvre-le avec{" "}
          <span className="font-medium">/gils</span>.
        </li>
        <li>
          Colle l'adresse <code className="rounded bg-muted px-1">{location.origin}</code> et un
          jeton créé ci-dessous, puis « Tester la connexion ».
        </li>
      </ol>

      <div className="flex flex-col gap-2 rounded-md border p-3">
        <div className="flex items-center justify-between gap-2">
          <span className="text-sm">Jetons</span>
          <Button type="button" size="sm" variant="outline" onClick={create}>
            <KeyRoundIcon />
            Créer un jeton
          </Button>
        </div>
        {fresh && (
          <div className="flex flex-col gap-1 rounded bg-muted p-2 text-xs">
            <span>Copie-le maintenant : il ne sera plus affiché.</span>
            <div className="flex items-center gap-2">
              <code className="truncate">{fresh}</code>
              <Button
                type="button"
                size="icon"
                variant="ghost"
                className="size-6"
                onClick={() => {
                  copy(fresh);
                }}
              >
                <CopyIcon />
              </Button>
            </div>
          </div>
        )}
        {tokens.length === 0 ? (
          <p className="text-xs text-muted-foreground">Aucun jeton.</p>
        ) : (
          <ul className="flex flex-col gap-1 text-xs">
            {tokens.map((t) => (
              <li key={t.id} className="flex items-center justify-between gap-2">
                <span>
                  {t.label} · créé {ago(t.createdAt)} ·{" "}
                  {t.lastUsedAt ? `utilisé ${ago(t.lastUsedAt)}` : "jamais utilisé"}
                </span>
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  className="size-6"
                  title="Révoquer"
                  onClick={() => {
                    revoke(t.id);
                  }}
                >
                  <Trash2Icon />
                </Button>
              </li>
            ))}
          </ul>
        )}
      </div>

      <p className="text-xs text-muted-foreground">
        {status.data?.updatedAt
          ? `Dernière synchronisation ${ago(status.data.updatedAt)}${
              character ? ` · ${character.name}` : ""
            } · ${String(status.data.retainers.length)} servant(s).`
          : "Aucune donnée reçue du plugin pour l'instant."}
      </p>
    </section>
  );
}
