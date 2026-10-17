// Réglages du compte (client web) : notifications de cet appareil, mot de
// passe, déconnexion. Pas de <form> : la section est dans celui des réglages.
import { useEffect, useState } from "react";
import { BellIcon, LogOutIcon } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { logout, username } from "@/lib/session";
import { errorMessage } from "@/lib/api";
import { serverCall } from "@/lib/backend";
import { disablePush, enablePush, PUSH_SUPPORTED, pushEnabled, testPush } from "@/lib/push";

export function AccountSection() {
  const [push, setPush] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");

  useEffect(() => {
    void pushEnabled().then(setPush);
  }, []);

  const togglePush = () => {
    setBusy(true);
    (push ? disablePush() : enablePush())
      .then(() => pushEnabled())
      .then((on) => {
        setPush(on);
        toast.success(on ? "Notifications activées sur cet appareil" : "Notifications désactivées");
      })
      .catch((e: unknown) => toast.error(errorMessage(e)))
      .finally(() => {
        setBusy(false);
      });
  };

  const changePassword = () => {
    serverCall("/api/auth/password", { current, new: next })
      .then(() => {
        toast.success("Mot de passe modifié");
        setCurrent("");
        setNext("");
      })
      .catch((e: unknown) => toast.error(errorMessage(e)));
  };

  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-[11px] font-semibold tracking-wider text-muted-foreground uppercase">
        Compte
      </h2>
      <p className="text-xs text-muted-foreground">
        Connecté en tant que <span className="font-medium text-foreground">{username()}</span>. Tes
        données (réglages, ventes, journal, liste de courses) sont sur le serveur : tu les retrouves
        sur tous tes appareils, et le suivi des ventes tourne même app fermée.
      </p>

      <div className="flex flex-col gap-2 rounded-md border p-3">
        <div className="flex items-center gap-2 text-sm">
          <BellIcon className="size-4" />
          Notifications sur cet appareil
        </div>
        {PUSH_SUPPORTED ? (
          <>
            <p className="text-xs text-muted-foreground">
              Alertes d'annonce dépassée et de vente, même quand l'app est fermée. À activer sur
              chaque appareil.
            </p>
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                size="sm"
                variant={push ? "outline" : "default"}
                disabled={busy || push === null}
                onClick={togglePush}
              >
                {push ? "Désactiver" : "Activer"}
              </Button>
              {push && (
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={() => {
                    testPush().catch((e: unknown) => toast.error(errorMessage(e)));
                  }}
                >
                  Envoyer un test
                </Button>
              )}
            </div>
          </>
        ) : (
          <p className="text-xs text-muted-foreground">
            Ce navigateur ne gère pas les notifications push. Sur iPhone, ajoute d'abord l'app à
            l'écran d'accueil.
          </p>
        )}
      </div>

      <div className="flex flex-col gap-2 rounded-md border p-3">
        <div className="text-sm">Changer de mot de passe</div>
        <div className="grid gap-2 sm:grid-cols-2">
          <div className="flex flex-col gap-1">
            <Label htmlFor="pw-current" className="text-xs">
              Actuel
            </Label>
            <Input
              id="pw-current"
              type="password"
              autoComplete="current-password"
              value={current}
              onChange={(e) => {
                setCurrent(e.target.value);
              }}
            />
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="pw-new" className="text-xs">
              Nouveau (8 caractères min.)
            </Label>
            <Input
              id="pw-new"
              type="password"
              autoComplete="new-password"
              value={next}
              onChange={(e) => {
                setNext(e.target.value);
              }}
            />
          </div>
        </div>
        <div>
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={!current || next.length < 8}
            onClick={changePassword}
          >
            Modifier
          </Button>
        </div>
      </div>

      <div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={() => {
            void logout();
          }}
        >
          <LogOutIcon />
          Se déconnecter
        </Button>
      </div>
    </section>
  );
}
