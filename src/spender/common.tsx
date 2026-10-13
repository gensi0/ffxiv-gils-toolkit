import type { ReactNode } from "react";
import { RefreshCwIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { errorMessage } from "@/lib/api";
import { useSettings, useWorlds } from "@/lib/queries";
import { usePrefs } from "@/spender/prefs";
import { useSpenderWorld } from "@/spender/world";

/** En-tête de colonne avec explication au survol. */
export function Hint({ label, hint }: { label: string; hint: string }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="cursor-help underline decoration-dotted underline-offset-2">{label}</span>
      </TooltipTrigger>
      <TooltipContent className="max-w-64">{hint}</TooltipContent>
    </Tooltip>
  );
}

export function WorldSelect() {
  const worlds = useWorlds();
  const settings = useSettings();
  const world = useSpenderWorld();
  const set = usePrefs((p) => p.set);
  const home = worlds.data?.find((w) => w.id === settings.data?.homeWorldId);
  // Serveurs du centre de données du serveur principal.
  const options = (worlds.data ?? [])
    .filter((w) => !home || w.dc === home.dc)
    .sort((a, b) => a.name.localeCompare(b.name));
  return (
    <Select
      value={world}
      onValueChange={(w) => {
        set({ world: w });
      }}
    >
      <SelectTrigger size="sm" className="w-36">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {options.map((w) => (
          <SelectItem key={w.id} value={w.name}>
            {w.name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

export function RefreshButton({ busy, onClick }: { busy: boolean; onClick: () => void }) {
  return (
    <Button variant="outline" size="sm" disabled={busy} onClick={onClick}>
      <RefreshCwIcon className={busy ? "animate-spin" : undefined} />
      Actualiser les prix
    </Button>
  );
}

export function Filter(props: {
  id: string;
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-center gap-1.5">
      <Checkbox
        id={props.id}
        checked={props.checked}
        onCheckedChange={(c) => {
          props.onChange(c === true);
        }}
      />
      <Label htmlFor={props.id}>{props.label}</Label>
    </div>
  );
}

export function PageBar({ children }: { children: ReactNode }) {
  return (
    <header className="flex h-11 shrink-0 items-center gap-3 border-b px-4">{children}</header>
  );
}

export function FilterBar({ children }: { children: ReactNode }) {
  return (
    <div className="flex flex-wrap items-center gap-4 border-b px-4 py-2 text-xs">{children}</div>
  );
}

/** Erreur, chargement, ou contenu. */
export function Loadable(props: {
  error: unknown;
  pending: boolean;
  onRetry: () => void;
  children: ReactNode;
}) {
  if (props.error) {
    return (
      <div className="flex items-center gap-3 p-4 text-xs text-destructive">
        {errorMessage(props.error)}
        <Button variant="outline" size="sm" onClick={props.onRetry}>
          Réessayer
        </Button>
      </div>
    );
  }
  if (props.pending) {
    return (
      <div className="flex flex-col gap-1 p-4">
        {Array.from({ length: 12 }, (_, i) => (
          <Skeleton key={i} className="h-8" />
        ))}
      </div>
    );
  }
  return <>{props.children}</>;
}
