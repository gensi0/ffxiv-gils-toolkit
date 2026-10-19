import { useMemo, useState, type ReactNode } from "react";
import { FolderOpenIcon, Loader2Icon } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { PageHeader } from "@/components/Page";
import {
  api,
  CRAFT_JOBS,
  GATHER_JOBS,
  errorMessage,
  LANGUAGES,
  RETAINER_CITIES,
  type Language,
  type RetainerCity,
  type Settings,
  type Theme,
  type World,
} from "@/lib/api";
import { cn } from "@/lib/utils";
import { IS_MOBILE } from "@/lib/compact";
import { IS_TAURI } from "@/lib/backend";
import { AccountSection } from "@/components/AccountSection";
import { PluginSection } from "@/components/PluginSection";
import { useAppInfo, useSaveSettings, useSettings, useTaxRates, useWorlds } from "@/lib/queries";

export function SettingsPage({ firstLaunch = false }: { firstLaunch?: boolean }) {
  const settings = useSettings();
  const worlds = useWorlds();
  const title = firstLaunch ? "Configuration" : "Paramètres";

  const error = settings.error ?? worlds.error;
  if (error) {
    return (
      <>
        <PageHeader title={title} />
        <div className="flex items-center gap-3 p-4 text-xs text-destructive">
          {errorMessage(error)}
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              void settings.refetch();
              void worlds.refetch();
            }}
          >
            Réessayer
          </Button>
        </div>
      </>
    );
  }
  if (!settings.data || !worlds.data) {
    return (
      <>
        <PageHeader title={title} />
        <div className="flex max-w-xl flex-col gap-3 p-4">
          {Array.from({ length: 6 }, (_, i) => (
            <Skeleton key={i} className="h-7" />
          ))}
        </div>
      </>
    );
  }
  return <SettingsForm initial={settings.data} worlds={worlds.data} firstLaunch={firstLaunch} />;
}

/** Text fields keep the raw input so the user can type freely; parsed on save. */
interface NumberFields {
  travelMinutesInRegion: string;
  travelMinutesCrossRegion: string;
  minProfitPerTrip: string;
  minUnitsPerDay: string;
  sellHorizonDays: string;
  maxDataAgeHours: string;
  /** Shown as a percentage; stored as a ratio. */
  minMarginPct: string;
  drillDownTopN: string;
  scanIntervalHours: string;
  watchNotifyProfit: string;
  salesTopRank: string;
}

const NUMBER_RULES: Record<keyof NumberFields, { integer: boolean; min: number }> = {
  travelMinutesInRegion: { integer: true, min: 1 },
  travelMinutesCrossRegion: { integer: true, min: 1 },
  minProfitPerTrip: { integer: true, min: 0 },
  minUnitsPerDay: { integer: false, min: 0 },
  sellHorizonDays: { integer: true, min: 1 },
  maxDataAgeHours: { integer: true, min: 1 },
  minMarginPct: { integer: false, min: 0 },
  drillDownTopN: { integer: true, min: 1 },
  scanIntervalHours: { integer: true, min: 0 },
  watchNotifyProfit: { integer: true, min: 0 },
  salesTopRank: { integer: true, min: 1 },
};

function SettingsForm(props: { initial: Settings; worlds: World[]; firstLaunch: boolean }) {
  const { initial, worlds, firstLaunch } = props;
  const save = useSaveSettings();
  const [homeWorldId, setHomeWorldId] = useState(initial.homeWorldId);
  const [retainerCity, setRetainerCity] = useState<RetainerCity>(initial.retainerCity);
  const [language, setLanguage] = useState<Language>(initial.language);
  const [extraRegions, setExtraRegions] = useState(initial.extraRegions);
  const [theme, setTheme] = useState<Theme>(initial.theme);
  const [fields, setFields] = useState<NumberFields>(() => ({
    travelMinutesInRegion: String(initial.travelMinutesInRegion),
    travelMinutesCrossRegion: String(initial.travelMinutesCrossRegion),
    minProfitPerTrip: String(initial.minProfitPerTrip),
    minUnitsPerDay: String(initial.minUnitsPerDay),
    sellHorizonDays: String(initial.sellHorizonDays),
    maxDataAgeHours: String(initial.maxDataAgeHours),
    minMarginPct: String(Math.round(initial.minMargin * 1000) / 10),
    drillDownTopN: String(initial.drillDownTopN),
    scanIntervalHours: String(initial.scanIntervalHours),
    watchNotifyProfit: String(initial.watchNotifyProfit),
    salesTopRank: String(initial.salesTopRank),
  }));
  const [notificationsEnabled, setNotificationsEnabled] = useState(initial.notificationsEnabled);
  const [retainerNames, setRetainerNames] = useState(initial.retainerNames.join(", "));
  const [lodestone, setLodestone] = useState(initial.lodestone);
  const [crafterLevels, setCrafterLevels] = useState<string[]>(() =>
    CRAFT_JOBS.map((_, i) => (initial.crafterLevels[i] ? String(initial.crafterLevels[i]) : "")),
  );
  const [gathererLevels, setGathererLevels] = useState<string[]>(() =>
    GATHER_JOBS.map((_, i) => (initial.gathererLevels[i] ? String(initial.gathererLevels[i]) : "")),
  );
  const [importing, setImporting] = useState(false);
  const importLodestone = () => {
    setImporting(true);
    api
      .craftLodestone(lodestone)
      .then((j) => {
        setCrafterLevels(j.levels.map((l) => (l > 0 ? String(l) : "")));
        setGathererLevels(j.gathererLevels.map((l) => (l > 0 ? String(l) : "")));
        toast.success(`Niveaux de ${j.character} importés : pense à enregistrer.`);
      })
      .catch((e: unknown) => toast.error(errorMessage(e)))
      .finally(() => {
        setImporting(false);
      });
  };

  const grouped = useMemo(() => groupWorlds(worlds), [worlds]);
  const home = worlds.find((w) => w.id === homeWorldId);
  const otherRegions = grouped.map(([region]) => region).filter((r) => r !== home?.region);

  const keys = Object.keys(NUMBER_RULES) as (keyof NumberFields)[];
  const parsed = Object.fromEntries(
    keys.map((k) => [k, parseNumber(fields[k], NUMBER_RULES[k])]),
  ) as Record<keyof NumberFields, number | null>;
  const valid = home !== undefined && Object.values(parsed).every((n) => n !== null);

  function onHomeChange(value: string) {
    const world = worlds.find((w) => w.id === Number(value));
    if (!world) return;
    setHomeWorldId(world.id);
    // The new home region can no longer be an "extra" source region.
    setExtraRegions((prev) => prev.filter((r) => r !== world.region));
  }

  function onSubmit(e: React.SyntheticEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!valid) return;
    const n = (k: Exclude<keyof NumberFields, "minMarginPct">) => parsed[k] ?? initial[k];
    const names = retainerNames
      .split(",")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    const namesChanged = names.join("|") !== initial.retainerNames.join("|");
    save.mutate(
      {
        homeWorldId,
        retainerCity,
        language,
        extraRegions,
        travelMinutesInRegion: n("travelMinutesInRegion"),
        travelMinutesCrossRegion: n("travelMinutesCrossRegion"),
        minProfitPerTrip: n("minProfitPerTrip"),
        minUnitsPerDay: n("minUnitsPerDay"),
        sellHorizonDays: n("sellHorizonDays"),
        maxDataAgeHours: n("maxDataAgeHours"),
        minMargin: parsed.minMarginPct !== null ? parsed.minMarginPct / 100 : initial.minMargin,
        drillDownTopN: n("drillDownTopN"),
        scanIntervalHours: n("scanIntervalHours"),
        notificationsEnabled,
        watchNotifyProfit: n("watchNotifyProfit"),
        salesTopRank: n("salesTopRank"),
        theme,
        retainerNames: names,
        lodestone: lodestone.trim(),
        crafterLevels: crafterLevels.some((l) => l.trim() !== "")
          ? crafterLevels.map((l) => Math.min(100, Math.max(0, Number.parseInt(l, 10) || 0)))
          : [],
        gathererLevels: gathererLevels.some((l) => l.trim() !== "")
          ? gathererLevels.map((l) => Math.min(100, Math.max(0, Number.parseInt(l, 10) || 0)))
          : [],
      },
      {
        onSuccess: () => {
          if (!firstLaunch) toast.success("Paramètres enregistrés");
          // Nouveaux servants : on cherche tout de suite leurs annonces.
          if (namesChanged && names.length > 0) void api.ventesDiscover().catch(() => undefined);
        },
        onError: (err) => toast.error(errorMessage(err)),
      },
    );
  }

  const numberInput = (key: keyof NumberFields, label: string, hint?: string) => (
    <Field label={label} htmlFor={key} hint={hint}>
      <Input
        id={key}
        inputMode={NUMBER_RULES[key].integer ? "numeric" : "decimal"}
        value={fields[key]}
        onChange={(e) => {
          setFields((f) => ({ ...f, [key]: e.target.value }));
        }}
        aria-invalid={parsed[key] === null}
      />
    </Field>
  );

  return (
    <>
      <PageHeader title={firstLaunch ? "Configuration" : "Paramètres"} />
      <form onSubmit={onSubmit} className="flex-1 overflow-y-auto">
        <div className="flex max-w-2xl flex-col gap-6 p-4">
          {firstLaunch && (
            <p className="text-xs text-muted-foreground">
              Choisis ton serveur principal pour commencer. Tout peut être modifié plus tard.
            </p>
          )}

          <Section title="Serveur principal">
            <div className="grid grid-cols-2 gap-3">
              <Field label="Serveur principal" htmlFor="home-world">
                <Select
                  value={homeWorldId === null ? "" : String(homeWorldId)}
                  onValueChange={onHomeChange}
                >
                  <SelectTrigger id="home-world" className="w-full">
                    <SelectValue placeholder="Choisir un serveur" />
                  </SelectTrigger>
                  <SelectContent className="max-h-80">
                    {grouped.map(([region, dcs]) =>
                      dcs.map(([dc, list]) => (
                        <SelectGroup key={dc}>
                          <SelectLabel>
                            {dc} · {REGION_LABELS[region] ?? region}
                          </SelectLabel>
                          {list.map((w) => (
                            <SelectItem key={w.id} value={String(w.id)}>
                              {w.name}
                            </SelectItem>
                          ))}
                        </SelectGroup>
                      )),
                    )}
                  </SelectContent>
                </Select>
              </Field>
              <Field
                label="Ville des servants"
                htmlFor="retainer-city"
                hint="Ville où vendent tes servants. Sa taxe de la semaine est appliquée à tous les calculs."
              >
                <Select
                  value={retainerCity}
                  onValueChange={(v) => {
                    setRetainerCity(v as RetainerCity);
                  }}
                >
                  <SelectTrigger id="retainer-city" className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {RETAINER_CITIES.map((c) => (
                      <SelectItem key={c} value={c}>
                        {CITY_LABELS[c]}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Field>
              <TaxRates worldId={homeWorldId} current={retainerCity} />
            </div>
            <Field label="Langue des noms d'objets" htmlFor="language">
              <ToggleGroup
                id="language"
                type="single"
                variant="outline"
                size="sm"
                value={language}
                onValueChange={(v) => {
                  if (v) setLanguage(v as Language);
                }}
                className="self-start"
              >
                {LANGUAGES.map((l) => (
                  <ToggleGroupItem key={l} value={l}>
                    {LANGUAGE_LABELS[l]}
                  </ToggleGroupItem>
                ))}
              </ToggleGroup>
            </Field>
          </Section>

          <Section title="Périmètre des sources">
            {home ? (
              <div className="flex flex-col gap-2">
                <RegionRow
                  region={home.region}
                  grouped={grouped}
                  checked
                  disabled
                  note="région principale"
                />
                {otherRegions.map((r) => (
                  <RegionRow
                    key={r}
                    region={r}
                    grouped={grouped}
                    checked={extraRegions.includes(r)}
                    note="autre région · trajet plus long"
                    onChange={(on) => {
                      setExtraRegions((prev) => (on ? [...prev, r] : prev.filter((x) => x !== r)));
                    }}
                  />
                ))}
                <p className="text-[11px] text-muted-foreground">
                  Ne cochez que les régions accessibles via le voyage entre centres de données.
                </p>
              </div>
            ) : (
              <p className="text-xs text-muted-foreground">Choisis d'abord un serveur principal.</p>
            )}
            <div className="grid grid-cols-2 gap-3">
              {numberInput("travelMinutesInRegion", "Durée du trajet, même région (min)")}
              {numberInput("travelMinutesCrossRegion", "Durée du trajet, autre région (min)")}
            </div>
          </Section>

          <Section title="Seuils">
            <div className="grid grid-cols-2 gap-3">
              {numberInput("minProfitPerTrip", "Bénéfice min. par trajet (gils)")}
              {numberInput("minUnitsPerDay", "Ventes min. sur le serveur principal (unités/jour)")}
              {numberInput(
                "sellHorizonDays",
                "Horizon de vente (jours)",
                "Durée pendant laquelle tu acceptes de garder du stock.",
              )}
              {numberInput(
                "maxDataAgeHours",
                "Âge max. des données (h)",
                "Ignore les serveurs (source et principal) dont les données sont plus anciennes.",
              )}
            </div>
          </Section>

          <Section title="Analyse">
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
              {numberInput("minMarginPct", "Marge min. (%)", "Préfiltre : bénéfice estimé / coût.")}
              {numberInput(
                "drillDownTopN",
                "Objets analysés en détail",
                "Meilleurs candidats du préfiltre.",
              )}
              {numberInput("scanIntervalHours", "Analyse auto toutes les (h)", "0 = désactivée.")}
            </div>
          </Section>

          <Section title="Surveillance du marché">
            <div className="flex items-center gap-2">
              <Checkbox
                id="notifications"
                checked={notificationsEnabled}
                onCheckedChange={(c) => {
                  setNotificationsEnabled(c === true);
                }}
              />
              <Label htmlFor="notifications">Notifications sur les fortes variations</Label>
            </div>
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
              {numberInput(
                "watchNotifyProfit",
                "Notifier au-delà de (gils / trajet)",
                "Valeur par défaut ; modifiable par objet.",
              )}
            </div>
          </Section>

          <Section title="Mes ventes">
            <Field
              label="Noms de mes servants"
              htmlFor="retainer-names"
              hint="Séparés par des virgules, tels qu'affichés en jeu. Sert à reconnaître tes annonces sur ton serveur principal pour suivre tes ventes."
            >
              <Input
                id="retainer-names"
                placeholder="Mogtaro, Lalanoir"
                value={retainerNames}
                onChange={(e) => {
                  setRetainerNames(e.target.value);
                }}
              />
            </Field>
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
              {numberInput(
                "salesTopRank",
                "Rang visé",
                "1 = la moins chère. Alerte et pastille quand une annonce sort de ce top.",
              )}
            </div>
          </Section>

          <Section title="Artisanat">
            <Field
              label="Lodestone"
              htmlFor="lodestone"
              hint="Lien de ta fiche personnage (ou son numéro) pour importer les niveaux de tes métiers d'artisan et de récolte. Le Lodestone se met à jour avec un peu de retard."
            >
              <div className="flex gap-2">
                <Input
                  id="lodestone"
                  placeholder="https://fr.finalfantasyxiv.com/lodestone/character/…"
                  value={lodestone}
                  onChange={(e) => {
                    setLodestone(e.target.value);
                  }}
                />
                <Button
                  type="button"
                  variant="outline"
                  disabled={importing || lodestone.trim() === ""}
                  onClick={importLodestone}
                >
                  {importing ? "Import…" : "Importer"}
                </Button>
              </div>
            </Field>
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
              {CRAFT_JOBS.map((job, i) => (
                <Field key={job} label={job} htmlFor={`craft-${String(i)}`}>
                  <Input
                    id={`craft-${String(i)}`}
                    inputMode="numeric"
                    placeholder="—"
                    value={crafterLevels[i]}
                    onChange={(e) => {
                      const v = e.target.value;
                      setCrafterLevels((l) => l.map((x, j) => (j === i ? v : x)));
                    }}
                  />
                </Field>
              ))}
            </div>
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
              {GATHER_JOBS.map((job, i) => (
                <Field key={job} label={job} htmlFor={`gather-${String(i)}`}>
                  <Input
                    id={`gather-${String(i)}`}
                    inputMode="numeric"
                    placeholder="—"
                    value={gathererLevels[i]}
                    onChange={(e) => {
                      const v = e.target.value;
                      setGathererLevels((l) => l.map((x, k) => (k === i ? v : x)));
                    }}
                  />
                </Field>
              ))}
            </div>
          </Section>

          <Section title="Apparence">
            <ToggleGroup
              type="single"
              variant="outline"
              size="sm"
              value={theme}
              onValueChange={(v) => {
                if (v) setTheme(v as Theme);
              }}
              className="self-start"
            >
              <ToggleGroupItem value="dark">Sombre</ToggleGroupItem>
              <ToggleGroupItem value="light">Clair</ToggleGroupItem>
            </ToggleGroup>
          </Section>

          <div>
            <Button type="submit" disabled={!valid || save.isPending}>
              {save.isPending && <Loader2Icon className="animate-spin" />}
              {firstLaunch ? "Commencer" : "Enregistrer"}
            </Button>
          </div>

          {!firstLaunch && !IS_TAURI && <AccountSection />}
          {!firstLaunch && !IS_TAURI && <PluginSection />}
          {!firstLaunch && <About />}
        </div>
      </form>
    </>
  );
}

function About() {
  const { data: info } = useAppInfo();
  return (
    <Section title="À propos">
      <div className="flex flex-col gap-1.5 text-xs text-muted-foreground">
        <p className="text-foreground">FFXIV Gils Toolkit {info && `v${info.version}`}</p>
        <p>Données de l'HV : universalis.app. Données des objets : XIVAPI.</p>
        <p>
          Cette application lit uniquement les données Universalis. Elle n'envoie jamais rien et
          n'intègre aucun uploader.
        </p>
        <p>Outil de fan, non affilié à Square Enix. FINAL FANTASY XIV © SQUARE ENIX CO., LTD.</p>
        <div className={cn("flex items-center gap-2 pt-1", (IS_MOBILE || !IS_TAURI) && "hidden")}>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => {
              api.openLogDir().catch((err: unknown) => toast.error(errorMessage(err)));
            }}
          >
            <FolderOpenIcon />
            Ouvrir le dossier des journaux
          </Button>
          {info && <span className="truncate text-[11px]">{info.dataDir}</span>}
        </div>
      </div>
    </Section>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-[11px] font-semibold tracking-wider text-muted-foreground uppercase">
        {title}
      </h2>
      {children}
    </section>
  );
}

function Field(props: { label: string; htmlFor: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <Label htmlFor={props.htmlFor}>{props.label}</Label>
      {props.children}
      {props.hint && <p className="text-[11px] text-muted-foreground">{props.hint}</p>}
    </div>
  );
}

type Grouped = [region: string, dcs: [dc: string, worlds: World[]][]][];

function RegionRow(props: {
  region: string;
  grouped: Grouped;
  checked: boolean;
  disabled?: boolean;
  note: string;
  onChange?: (checked: boolean) => void;
}) {
  const id = `region-${props.region}`;
  const dcs = props.grouped.find(([r]) => r === props.region)?.[1].map(([dc]) => dc) ?? [];
  return (
    <div className="flex items-center gap-2">
      <Checkbox
        id={id}
        checked={props.checked}
        disabled={props.disabled}
        onCheckedChange={(c) => props.onChange?.(c === true)}
      />
      <Label htmlFor={id}>{REGION_LABELS[props.region] ?? props.region}</Label>
      <span className="text-[11px] text-muted-foreground">
        {dcs.join(", ")} · {props.note}
      </span>
    </div>
  );
}

const LANGUAGE_LABELS: Record<Language, string> = {
  en: "English",
  fr: "Français",
  de: "Deutsch",
  ja: "日本語",
};

/** Taxes de vente de la semaine sur le serveur principal, la moins chère en vert. */
function TaxRates({ worldId, current }: { worldId: number | null; current: RetainerCity }) {
  const rates = useTaxRates(worldId);
  if (worldId === null) return null;
  if (rates.isPending) return <Skeleton className="h-10 w-full" />;
  if (rates.isError) {
    return (
      <p className="text-[11px] text-muted-foreground">
        Taxes de la semaine indisponibles (Universalis injoignable) : 5 % est utilisé par défaut.
      </p>
    );
  }
  const values = RETAINER_CITIES.map((c) => rates.data[c]).filter(
    (v): v is number => v !== undefined,
  );
  const best = values.length > 0 ? Math.min(...values) : null;
  return (
    <div className="col-span-full flex flex-col gap-1.5">
      <span className="text-xs font-medium">Taxes de vente cette semaine</span>
      <div className="flex flex-wrap gap-1.5">
        {RETAINER_CITIES.map((c) => {
          const v = rates.data[c];
          return (
            <span
              key={c}
              className={cn(
                "rounded-md border px-2 py-1 text-[11px] tabular-nums",
                v === best && "border-emerald-500/50 text-emerald-600 dark:text-emerald-400",
                c === current && "bg-muted font-semibold",
              )}
            >
              {CITY_LABELS[c]} {v === undefined ? "?" : `${String(v)} %`}
            </span>
          );
        })}
      </div>
      <span className="text-[11px] text-muted-foreground">
        Les taux changent chaque semaine selon le volume des ventes. En gras : ta ville actuelle ;
        en vert : la moins taxée.
      </span>
    </div>
  );
}

const CITY_LABELS: Record<RetainerCity, string> = {
  "Limsa Lominsa": "Limsa Lominsa",
  Gridania: "Gridania",
  "Ul'dah": "Ul'dah",
  Ishgard: "Ishgard",
  Kugane: "Kugane",
  Crystarium: "Cristarium",
  "Old Sharlayan": "Vieille Sharlayan",
  Tuliyollal: "Tuliyollal",
};

/** Display names for region identifiers (values stay untranslated). */
const REGION_LABELS: Record<string, string> = {
  Europe: "Europe",
  "North-America": "Amérique du Nord",
  Oceania: "Océanie",
  Japan: "Japon",
};

const REGION_ORDER = ["Europe", "North-America", "Oceania", "Japan"];
const regionRank = (r: string) => {
  const i = REGION_ORDER.indexOf(r);
  return i === -1 ? REGION_ORDER.length : i;
};

/** Worlds grouped by region then DC, in a stable display order. */
function groupWorlds(worlds: World[]): Grouped {
  const regions = new Map<string, Map<string, World[]>>();
  for (const w of worlds) {
    let dcs = regions.get(w.region);
    if (!dcs) regions.set(w.region, (dcs = new Map<string, World[]>()));
    const list = dcs.get(w.dc);
    if (list) list.push(w);
    else dcs.set(w.dc, [w]);
  }
  return [...regions.entries()]
    .sort(([a], [b]) => regionRank(a) - regionRank(b) || a.localeCompare(b))
    .map(([region, dcs]) => [region, [...dcs.entries()].sort(([a], [b]) => a.localeCompare(b))]);
}

function parseNumber(raw: string, rule: { integer: boolean; min: number }): number | null {
  // Integers accept thousands separators ("10 000", "10,000"); decimals
  // accept a comma as decimal mark ("1,5").
  const compact = raw.trim().replace(/[\s_]/g, "");
  const text = rule.integer ? compact.replace(/,/g, "") : compact.replace(",", ".");
  const n = Number(text);
  if (text === "" || !Number.isFinite(n) || n < rule.min) return null;
  if (rule.integer && !Number.isInteger(n)) return null;
  return n;
}
