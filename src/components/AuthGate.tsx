// Connexion au compte (client web) : formulaire de connexion / inscription et
// garde qui n'affiche l'app qu'une fois connecté.
import { type ReactNode, type SyntheticEvent, useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import {
  AuthError,
  IS_TAURI,
  onAuthLost,
  serverCall,
  serverGet,
  startEvents,
  stopEvents,
} from "@/lib/backend";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/utils";
import { setUsername } from "@/lib/session";
import { hydrateAccountStores } from "@/lib/accountStorage";

interface Me {
  username: string;
}

export function AuthGate({ children }: { children: ReactNode }) {
  const qc = useQueryClient();
  const [me, setMe] = useState<Me | null | undefined>(IS_TAURI ? { username: "" } : undefined);
  // Données du compte (liste de courses…) chargées : l'app peut s'afficher.
  const [ready, setReady] = useState(IS_TAURI);

  useEffect(() => {
    if (IS_TAURI) return;
    serverGet<Me>("/api/auth/me")
      .then(setMe)
      .catch((e: unknown) => {
        // Serveur injoignable : on réessaie en affichant le formulaire.
        if (!(e instanceof AuthError)) console.warn(e);
        setMe(null);
      });
    return onAuthLost(() => {
      stopEvents();
      qc.clear();
      setMe(null);
    });
  }, [qc]);

  useEffect(() => {
    if (me && !IS_TAURI) {
      setUsername(me.username);
      startEvents();
      void hydrateAccountStores().finally(() => {
        setReady(true);
      });
    }
  }, [me]);

  if (me === undefined || (me && !ready)) return <Skeleton className="m-4 h-7 w-64" />;
  if (me === null)
    return (
      <LoginPage
        onDone={(m) => {
          qc.clear();
          setMe(m);
        }}
      />
    );
  return children;
}

function LoginPage({ onDone }: { onDone: (me: Me) => void }) {
  const [mode, setMode] = useState<"login" | "register">("login");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [invite, setInvite] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    const body = mode === "login" ? { username, password } : { username, password, invite };
    serverCall<Me>(`/api/auth/${mode}`, body)
      .then(onDone)
      .catch((err: unknown) => {
        setError(errorMessage(err));
      })
      .finally(() => {
        setBusy(false);
      });
  };

  return (
    <div className="flex h-full items-center justify-center overflow-y-auto p-4">
      <form
        onSubmit={submit}
        className="flex w-full max-w-sm flex-col gap-4 rounded-lg border bg-card p-6"
      >
        <div className="flex flex-col items-center gap-2 text-center">
          <img src="/icon-192.png" alt="" className="size-14" />
          <h1 className="text-lg font-semibold">FFXIV Gils Toolkit</h1>
        </div>
        <div className="grid grid-cols-2 rounded-md bg-muted p-1 text-sm">
          {(["login", "register"] as const).map((m) => (
            <button
              key={m}
              type="button"
              className={cn(
                "rounded py-1",
                mode === m ? "bg-background shadow-sm" : "text-muted-foreground",
              )}
              onClick={() => {
                setMode(m);
                setError(null);
              }}
            >
              {m === "login" ? "Connexion" : "Créer un compte"}
            </button>
          ))}
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="username">Identifiant</Label>
          <Input
            id="username"
            autoComplete="username"
            autoCapitalize="none"
            value={username}
            onChange={(e) => {
              setUsername(e.target.value);
            }}
            required
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="password">Mot de passe</Label>
          <Input
            id="password"
            type="password"
            autoComplete={mode === "login" ? "current-password" : "new-password"}
            minLength={mode === "register" ? 8 : undefined}
            value={password}
            onChange={(e) => {
              setPassword(e.target.value);
            }}
            required
          />
        </div>
        {mode === "register" && (
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="invite">Code d'invitation</Label>
            <Input
              id="invite"
              autoCapitalize="none"
              value={invite}
              onChange={(e) => {
                setInvite(e.target.value);
              }}
              required
            />
          </div>
        )}
        {error && <p className="text-sm text-red-500">{error}</p>}
        <Button type="submit" disabled={busy}>
          {mode === "login" ? "Se connecter" : "Créer le compte"}
        </Button>
      </form>
    </div>
  );
}
