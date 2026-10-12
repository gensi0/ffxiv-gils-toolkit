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
  errorMessage,
  LANGUAGES,
  RETAINER_CITIES,
  type Language,
  type RetainerCity,
  type Settings,
  type Theme,
  type World,
} from "@/lib/api";
import { useAppInfo, useSaveSettings, useSettings, useWorlds } from "@/lib/queries";

export function SettingsPage({ firstLaunch = false }: { firstLaunch?: boolean }) {
  const settings = useSettings();
  const worlds = useWorlds();
  const title = firstLaunch ? "Setup" : "Settings";

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
            Retry
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
  }));
  const [notificationsEnabled, setNotificationsEnabled] = useState(initial.notificationsEnabled);

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
        theme,
      },
      {
        onSuccess: () => {
          if (!firstLaunch) toast.success("Settings saved");
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
      <PageHeader title={firstLaunch ? "Setup" : "Settings"} />
      <form onSubmit={onSubmit} className="flex-1 overflow-y-auto">
        <div className="flex max-w-2xl flex-col gap-6 p-4">
          {firstLaunch && (
            <p className="text-xs text-muted-foreground">
              Pick your home world to start. Everything here can be changed later.
            </p>
          )}

          <Section title="Home">
            <div className="grid grid-cols-2 gap-3">
              <Field label="Home world" htmlFor="home-world">
                <Select
                  value={homeWorldId === null ? "" : String(homeWorldId)}
                  onValueChange={onHomeChange}
                >
                  <SelectTrigger id="home-world" className="w-full">
                    <SelectValue placeholder="Select a world" />
                  </SelectTrigger>
                  <SelectContent className="max-h-80">
                    {grouped.map(([region, dcs]) =>
                      dcs.map(([dc, list]) => (
                        <SelectGroup key={dc}>
                          <SelectLabel>
                            {dc} · {region}
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
                label="Retainer city"
                htmlFor="retainer-city"
                hint="Where your selling retainers are. Sets the seller tax (0–5%)."
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
                        {c}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Field>
            </div>
            <Field label="Item name language" htmlFor="language">
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
                  <ToggleGroupItem key={l} value={l} className="uppercase">
                    {l}
                  </ToggleGroupItem>
                ))}
              </ToggleGroup>
            </Field>
          </Section>

          <Section title="Source scope">
            {home ? (
              <div className="flex flex-col gap-2">
                <RegionRow
                  region={home.region}
                  grouped={grouped}
                  checked
                  disabled
                  note="home region"
                />
                {otherRegions.map((r) => (
                  <RegionRow
                    key={r}
                    region={r}
                    grouped={grouped}
                    checked={extraRegions.includes(r)}
                    note="cross-region · slower travel"
                    onChange={(on) => {
                      setExtraRegions((prev) => (on ? [...prev, r] : prev.filter((x) => x !== r)));
                    }}
                  />
                ))}
                <p className="text-[11px] text-muted-foreground">
                  Only tick regions your Data Center Travel currently allows.
                </p>
              </div>
            ) : (
              <p className="text-xs text-muted-foreground">Pick a home world first.</p>
            )}
            <div className="grid grid-cols-2 gap-3">
              {numberInput("travelMinutesInRegion", "Trip time, in region (min)")}
              {numberInput("travelMinutesCrossRegion", "Trip time, cross-region (min)")}
            </div>
          </Section>

          <Section title="Thresholds">
            <div className="grid grid-cols-2 gap-3">
              {numberInput("minProfitPerTrip", "Min profit per trip (gil)")}
              {numberInput("minUnitsPerDay", "Min home sell rate (units/day)")}
              {numberInput(
                "sellHorizonDays",
                "Sell horizon (days)",
                "How long you accept holding stock.",
              )}
              {numberInput(
                "maxDataAgeHours",
                "Data freshness limit (h)",
                "Skip source and home worlds with older uploads.",
              )}
            </div>
          </Section>

          <Section title="Scan">
            <div className="grid grid-cols-3 gap-3">
              {numberInput("minMarginPct", "Min margin (%)", "Prefilter: estimated profit / cost.")}
              {numberInput("drillDownTopN", "Items analyzed in depth", "Top prefilter candidates.")}
              {numberInput("scanIntervalHours", "Auto-scan every (h)", "0 = off.")}
            </div>
          </Section>

          <Section title="Watchlist">
            <div className="flex items-center gap-2">
              <Checkbox
                id="notifications"
                checked={notificationsEnabled}
                onCheckedChange={(c) => {
                  setNotificationsEnabled(c === true);
                }}
              />
              <Label htmlFor="notifications">Desktop notifications on big movers</Label>
            </div>
            <div className="grid grid-cols-3 gap-3">
              {numberInput(
                "watchNotifyProfit",
                "Notify above (gil / trip)",
                "Default; each item can override it.",
              )}
            </div>
          </Section>

          <Section title="Appearance">
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
              <ToggleGroupItem value="dark">Dark</ToggleGroupItem>
              <ToggleGroupItem value="light">Light</ToggleGroupItem>
            </ToggleGroup>
          </Section>

          <div>
            <Button type="submit" disabled={!valid || save.isPending}>
              {save.isPending && <Loader2Icon className="animate-spin" />}
              {firstLaunch ? "Start" : "Save"}
            </Button>
          </div>

          {!firstLaunch && <About />}
        </div>
      </form>
    </>
  );
}

function About() {
  const { data: info } = useAppInfo();
  return (
    <Section title="About">
      <div className="flex flex-col gap-1.5 text-xs text-muted-foreground">
        <p className="text-foreground">FFXIV Trader {info && `v${info.version}`}</p>
        <p>Market data from universalis.app. Item data from XIVAPI.</p>
        <p>
          This app only reads Universalis data. It never uploads anything and has no uploader
          integration.
        </p>
        <p>Fan tool, not affiliated with Square Enix. FINAL FANTASY XIV © SQUARE ENIX CO., LTD.</p>
        <div className="flex items-center gap-2 pt-1">
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => {
              api.openLogDir().catch((err: unknown) => toast.error(errorMessage(err)));
            }}
          >
            <FolderOpenIcon />
            Open log folder
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
      <Label htmlFor={id}>{props.region}</Label>
      <span className="text-[11px] text-muted-foreground">
        {dcs.join(", ")} · {props.note}
      </span>
    </div>
  );
}

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
