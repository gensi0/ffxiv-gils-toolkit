import { useMemo, useState } from "react";
import { Popover } from "radix-ui";
import { CheckIcon, ChevronDownIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { DataTable } from "@/components/DataTable";
import { columnHelper } from "@/lib/table";
import { EmptyState, PageHeader } from "@/components/Page";
import { Age, ConfidenceBadge, ItemIcon, QualityTag } from "@/components/market";
import { ScanButton, ScanProgressBar } from "@/components/ScanControls";
import { TripCards } from "@/components/TripCards";
import { api, errorMessage, type Candidate, type Opportunity, type ScanResults } from "@/lib/api";
import { ago, decimal, gil } from "@/lib/format";
import { isRunning, useScanCandidates, useScanResults, useWorlds } from "@/lib/queries";
import { useLive } from "@/store/live";
import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { useNav } from "@/store/nav";
import { useScanFilters } from "@/store/scanFilters";

export function ScanPage() {
  const scanId = useNav((s) => s.scanId);
  const openScan = useNav((s) => s.openScan);
  const results = useScanResults(scanId);
  const [tab, setTab] = useState<"opportunities" | "candidates">("opportunities");

  return (
    <>
      <PageHeader title="Scan">
        {scanId !== null && (
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              openScan(null);
            }}
          >
            Show latest
          </Button>
        )}
        <ScanButton />
      </PageHeader>
      <ScanProgressBar />
      {results.isError ? (
        <div className="flex items-center gap-3 p-4 text-xs text-destructive">
          {errorMessage(results.error)}
          <Button variant="outline" size="sm" onClick={() => void results.refetch()}>
            Retry
          </Button>
        </div>
      ) : results.isPending ? (
        <div className="flex flex-col gap-1 p-4">
          {Array.from({ length: 10 }, (_, i) => (
            <Skeleton key={i} className="h-8" />
          ))}
        </div>
      ) : results.data === null ? (
        <EmptyState message="No completed scan yet." action={<ScanButton />} />
      ) : (
        <>
          <RunSummary r={results.data} tab={tab} onTab={setTab} />
          {tab === "opportunities" ? (
            <OpportunityView r={results.data} />
          ) : (
            <CandidateView scanId={results.data.run.scanId} />
          )}
        </>
      )}
    </>
  );
}

function RunSummary(props: {
  r: ScanResults;
  tab: string;
  onTab: (t: "opportunities" | "candidates") => void;
}) {
  const { run } = props.r;
  return (
    <div className="flex items-center gap-4 border-b px-4 py-1.5 text-[11px] text-muted-foreground">
      <Tabs
        value={props.tab}
        onValueChange={(v) => {
          props.onTab(v as "opportunities" | "candidates");
        }}
      >
        <TabsList className="h-7">
          <TabsTrigger value="opportunities" className="text-xs">
            Opportunities ({run.opportunitiesFound ?? 0})
          </TabsTrigger>
          <TabsTrigger value="candidates" className="text-xs">
            Prefilter candidates
          </TabsTrigger>
        </TabsList>
      </Tabs>
      <span>
        Scanned {ago(run.startedAt)} · {gil(run.itemsScanned)} items · home {props.r.homeWorld}
      </span>
      {(run.itemsFailed ?? 0) > 0 && (
        <span className="flex items-center gap-2 text-amber-600 dark:text-amber-400">
          {run.itemsFailed} items failed to fetch
          <RetryButton scanId={run.scanId} />
        </span>
      )}
    </div>
  );
}

const oc = columnHelper<Opportunity>();
const opportunityColumns = [
  oc.accessor((o) => o.item.name, {
    id: "item",
    header: "Item",
    cell: (c) => (
      <span className="flex items-center gap-2">
        <ItemIcon path={c.row.original.item.iconPath} className="size-5" />
        <span className="max-w-64 truncate">{c.getValue()}</span>
      </span>
    ),
  }),
  oc.accessor("quality", {
    header: "Q",
    cell: (c) => <QualityTag quality={c.getValue()} soldAs={c.row.original.soldAs} />,
  }),
  oc.accessor("sourceWorld", {
    header: "Source",
    cell: (c) => (
      <span>
        {c.getValue()}
        <span className="text-muted-foreground"> · {c.row.original.sourceDc}</span>
        {c.row.original.crossRegion && (
          <span className="ml-1 text-[11px] text-amber-600 dark:text-amber-400">cross-region</span>
        )}
      </span>
    ),
  }),
  oc.accessor("sourceUnitPrice", {
    header: "Buy at",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  oc.accessor("unitsToBuy", {
    header: "Units",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  oc.accessor("expectedSellPrice", {
    header: "Sell at",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  oc.accessor("unitProfit", {
    header: "Profit / unit",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  oc.accessor("profitPerTrip", {
    header: "Profit / trip",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  oc.accessor("dailyProfit", {
    header: "Gil / day",
    cell: (c) => <span className="font-medium">{gil(c.getValue())}</span>,
    meta: { numeric: true },
  }),
  oc.accessor("homeUnitsPerDay", {
    header: "Sold / day",
    cell: (c) => decimal(c.getValue()),
    meta: { numeric: true },
  }),
  oc.accessor("confidence", {
    header: "Confidence",
    cell: (c) => <ConfidenceBadge value={c.getValue()} />,
    sortFn: "confidence",
  }),
  oc.accessor("sourceUploadMs", {
    header: "Data age",
    cell: (c) => (
      <span className={cn(c.row.original.isStale && "text-muted-foreground/60")}>
        <Age ms={c.getValue()} />
      </span>
    ),
    meta: { numeric: true },
  }),
];

const CONFIDENCE_OPTIONS = ["HOT", "OK", "COMPETITIVE", "SATURATED", "SLOW"];

function OpportunityView({ r }: { r: ScanResults }) {
  const openItem = useNav((s) => s.openItem);
  const worlds = useWorlds();

  // Persistent filters & sorting via Zustand store
  const {
    category,
    setCategory,
    selRegions,
    setSelRegions,
    selDcs,
    setSelDcs,
    selWorlds,
    setSelWorlds,
    selConfidence,
    setSelConfidence,
    hqOnly,
    setHqOnly,
    minProfit,
    setMinProfit,
    sorting,
    setSorting,
  } = useScanFilters();

  const [grouped, setGrouped] = useState(false);

  // Build DC→region lookup from worlds data
  const dcToRegion = useMemo(() => {
    const map = new Map<string, string>();
    for (const w of worlds.data ?? []) map.set(w.dc, w.region);
    return map;
  }, [worlds.data]);

  const categories = useMemo(
    () =>
      [
        ...new Set(r.opportunities.map((o) => o.item.category).filter((c): c is string => !!c)),
      ].sort(),
    [r.opportunities],
  );

  const regions = useMemo(
    () => [...new Set(r.opportunities.map((o) => dcToRegion.get(o.sourceDc) ?? "Unknown"))].sort(),
    [r.opportunities, dcToRegion],
  );

  // DCs filtered by selected regions
  const dcs = useMemo(() => {
    const all = [...new Set(r.opportunities.map((o) => o.sourceDc))].sort();
    if (selRegions.length === 0) return all;
    return all.filter((d) => selRegions.includes(dcToRegion.get(d) ?? "Unknown"));
  }, [r.opportunities, selRegions, dcToRegion]);

  // Worlds filtered by selected DCs (and regions)
  const availableWorlds = useMemo(() => {
    const all = [...new Set(r.opportunities.map((o) => o.sourceWorld))].sort();
    if (selDcs.length > 0)
      return all.filter((w) => {
        const opp = r.opportunities.find((o) => o.sourceWorld === w);
        return opp ? selDcs.includes(opp.sourceDc) : false;
      });
    if (selRegions.length > 0)
      return all.filter((w) => {
        const opp = r.opportunities.find((o) => o.sourceWorld === w);
        return opp ? selRegions.includes(dcToRegion.get(opp.sourceDc) ?? "Unknown") : false;
      });
    return all;
  }, [r.opportunities, selDcs, selRegions, dcToRegion]);

  // Cascade: when regions change, drop DCs/worlds no longer valid
  const handleRegionChange = (v: string[]) => {
    setSelRegions(v);
    if (v.length > 0) {
      setSelDcs(selDcs.filter((d) => v.includes(dcToRegion.get(d) ?? "Unknown")));
      setSelWorlds([]);
    }
  };
  const handleDcChange = (v: string[]) => {
    setSelDcs(v);
    if (v.length > 0) setSelWorlds([]);
  };

  const minProfitValue = Number(minProfit.replace(/[\s,_]/g, "")) || 0;
  const rows = useMemo(
    () =>
      r.opportunities.filter(
        (o) =>
          (category.length === 0 || category.includes(o.item.category ?? "")) &&
          (selRegions.length === 0 ||
            selRegions.includes(dcToRegion.get(o.sourceDc) ?? "Unknown")) &&
          (selDcs.length === 0 || selDcs.includes(o.sourceDc)) &&
          (selWorlds.length === 0 || selWorlds.includes(o.sourceWorld)) &&
          (selConfidence.length === 0 || selConfidence.includes(o.confidence)) &&
          (!hqOnly || o.quality === "HQ") &&
          o.profitPerTrip >= minProfitValue,
      ),
    [
      r.opportunities,
      category,
      selRegions,
      selDcs,
      selWorlds,
      selConfidence,
      hqOnly,
      minProfitValue,
      dcToRegion,
    ],
  );

  if (r.opportunities.length === 0) {
    return <EmptyState message="This scan found no opportunity above your thresholds." />;
  }
  return (
    <>
      <div className="flex flex-wrap items-center gap-3 border-b px-4 py-2 text-xs">
        <div className="flex items-center gap-1.5">
          <Checkbox
            id="group-by-world"
            checked={grouped}
            onCheckedChange={(c) => {
              setGrouped(c === true);
            }}
          />
          <Label htmlFor="group-by-world">Group by source world</Label>
        </div>
        {grouped ? (
          <span className="text-muted-foreground">
            One card per world, best trips first. Filters do not apply to trips.
          </span>
        ) : (
          <>
            <MultiSelect
              value={category}
              onChange={setCategory}
              all="All categories"
              options={categories}
            />
            <MultiSelect
              value={selRegions}
              onChange={handleRegionChange}
              all="All regions"
              options={regions}
            />
            <MultiSelect value={selDcs} onChange={handleDcChange} all="All DCs" options={dcs} />
            <MultiSelect
              value={selWorlds}
              onChange={setSelWorlds}
              all="All servers"
              options={availableWorlds}
            />
            <MultiSelect
              value={selConfidence}
              onChange={setSelConfidence}
              all="All confidence"
              options={CONFIDENCE_OPTIONS}
            />
            <Input
              placeholder="Min profit / trip"
              inputMode="numeric"
              value={minProfit}
              onChange={(e) => {
                setMinProfit(e.target.value);
              }}
              className="h-7 w-36"
            />
            <div className="flex items-center gap-1.5">
              <Checkbox
                id="hq-only"
                checked={hqOnly}
                onCheckedChange={(c) => {
                  setHqOnly(c === true);
                }}
              />
              <Label htmlFor="hq-only">HQ only</Label>
            </div>
            <span className="ml-auto text-muted-foreground">
              {rows.length} of {r.opportunities.length}
            </span>
          </>
        )}
      </div>
      {grouped ? (
        <TripCards r={r} />
      ) : (
        <DataTable
          columns={opportunityColumns}
          data={rows}
          sorting={sorting}
          onSortingChange={setSorting}
          getRowId={(o) => `${o.item.id}-${o.quality}-${o.sourceWorldId}`}
          onRowClick={(o) => {
            openItem(o.item.id);
          }}
        />
      )}
    </>
  );
}

function MultiSelect({
  all,
  options,
  value,
  onChange,
}: {
  all: string;
  options: string[];
  value: string[];
  onChange: (v: string[]) => void;
}) {
  const toggle = (opt: string) => {
    onChange(value.includes(opt) ? value.filter((v) => v !== opt) : [...value, opt]);
  };
  const label =
    value.length === 0 ? all : value.length === 1 ? value[0] : `${value.length} selected`;

  return (
    <Popover.Root>
      <Popover.Trigger asChild>
        <Button
          variant="outline"
          size="sm"
          className={cn(
            "h-7 w-40 justify-between px-2 text-xs font-normal",
            value.length > 0 && "border-primary/60 text-foreground",
          )}
        >
          <span className="truncate">{label}</span>
          <ChevronDownIcon className="ml-1 size-3 shrink-0 text-muted-foreground" />
        </Button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="start"
          sideOffset={4}
          className="z-50 max-h-72 w-48 overflow-y-auto rounded-lg border bg-popover p-1 shadow-md"
        >
          {value.length > 0 && (
            <>
              <button
                onClick={() => {
                  onChange([]);
                }}
                className="flex w-full items-center gap-2 rounded px-2 py-1 text-xs text-muted-foreground hover:bg-muted hover:text-foreground"
              >
                Clear selection
              </button>
              <Separator className="my-1" />
            </>
          )}
          {options.map((opt) => (
            <button
              key={opt}
              onClick={() => {
                toggle(opt);
              }}
              className="flex w-full items-center gap-2 rounded px-2 py-1 text-xs hover:bg-muted"
            >
              <span className="flex size-3.5 shrink-0 items-center justify-center rounded-sm border border-muted-foreground/40">
                {value.includes(opt) && <CheckIcon className="size-3" />}
              </span>
              <span className="truncate">{opt}</span>
            </button>
          ))}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

const cc = columnHelper<Candidate>();
const candidateColumns = [
  cc.accessor((c) => c.item.name, {
    id: "item",
    header: "Item",
    cell: (c) => (
      <span className="flex items-center gap-2">
        <ItemIcon path={c.row.original.item.iconPath} className="size-5" />
        <span className="max-w-64 truncate">{c.getValue()}</span>
      </span>
    ),
  }),
  cc.accessor("quality", {
    header: "Q",
    cell: (c) => <QualityTag quality={c.getValue()} soldAs={c.row.original.soldAs} />,
  }),
  cc.accessor("sourceWorld", {
    header: "Cheapest in DC",
    cell: (c) => (
      <span>
        {c.getValue()} <span className="text-muted-foreground">· {c.row.original.sourceDc}</span>
      </span>
    ),
  }),
  cc.accessor("sourceMinPrice", {
    header: "Min price",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  cc.accessor("estSellPrice", {
    header: "Est. sell",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  cc.accessor("estProfit", {
    header: "Est. profit / unit",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  cc.accessor("homeUnitsPerDay", {
    header: "Sold / day",
    cell: (c) => decimal(c.getValue()),
    meta: { numeric: true },
  }),
  cc.accessor("estScore", {
    header: "Score",
    cell: (c) => gil(c.getValue()),
    meta: { numeric: true },
  }),
  cc.accessor("sourceUploadMs", {
    header: "Data age",
    cell: (c) => <Age ms={c.getValue()} />,
    meta: { numeric: true },
  }),
];

function CandidateView({ scanId }: { scanId: string }) {
  const candidates = useScanCandidates(scanId);
  const openItem = useNav((s) => s.openItem);
  if (candidates.isError) {
    return (
      <div className="flex items-center gap-3 p-4 text-xs text-destructive">
        {errorMessage(candidates.error)}
        <Button variant="outline" size="sm" onClick={() => void candidates.refetch()}>
          Retry
        </Button>
      </div>
    );
  }
  if (!candidates.data) {
    return (
      <div className="flex flex-col gap-1 p-4">
        {Array.from({ length: 10 }, (_, i) => (
          <Skeleton key={i} className="h-8" />
        ))}
      </div>
    );
  }
  if (candidates.data.length === 0) return <EmptyState message="No item passed the prefilter." />;
  return (
    <>
      <p className="border-b px-4 py-1.5 text-[11px] text-muted-foreground">
        Estimates from aggregated data only (top 300 by score). Opportunities are the drilled-down
        subset.
      </p>
      <DataTable
        columns={candidateColumns}
        data={candidates.data}
        initialSorting={[{ id: "estScore", desc: true }]}
        getRowId={(c) => `${c.item.id}-${c.quality}-${c.sourceDc}`}
        onRowClick={(c) => {
          openItem(c.item.id);
        }}
      />
    </>
  );
}

function RetryButton({ scanId }: { scanId: string }) {
  const running = isRunning(useLive((s) => s.scan));
  const retry = useMutation({
    mutationFn: () => api.retryFailedScan(scanId),
    onError: (e) => toast.error(errorMessage(e)),
  });
  return (
    <Button
      variant="outline"
      size="xs"
      disabled={running || retry.isPending}
      onClick={() => {
        retry.mutate();
      }}
    >
      Retry
    </Button>
  );
}
