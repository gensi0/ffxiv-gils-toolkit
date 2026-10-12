import { useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
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
import { isRunning, useScanCandidates, useScanResults } from "@/lib/queries";
import { useLive } from "@/store/live";
import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { useNav } from "@/store/nav";

const ALL = "__all__";

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

function OpportunityView({ r }: { r: ScanResults }) {
  const openItem = useNav((s) => s.openItem);
  const [category, setCategory] = useState(ALL);
  const [dc, setDc] = useState(ALL);
  const [hqOnly, setHqOnly] = useState(false);
  const [minProfit, setMinProfit] = useState("");
  const [grouped, setGrouped] = useState(false);

  const categories = useMemo(
    () =>
      [
        ...new Set(r.opportunities.map((o) => o.item.category).filter((c): c is string => !!c)),
      ].sort(),
    [r.opportunities],
  );
  const dcs = useMemo(
    () => [...new Set(r.opportunities.map((o) => o.sourceDc))].sort(),
    [r.opportunities],
  );
  const minProfitValue = Number(minProfit.replace(/[\s,_]/g, "")) || 0;
  const rows = useMemo(
    () =>
      r.opportunities.filter(
        (o) =>
          (category === ALL || o.item.category === category) &&
          (dc === ALL || o.sourceDc === dc) &&
          (!hqOnly || o.quality === "HQ") &&
          o.profitPerTrip >= minProfitValue,
      ),
    [r.opportunities, category, dc, hqOnly, minProfitValue],
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
            <FilterSelect
              value={category}
              onChange={setCategory}
              all="All categories"
              options={categories}
            />
            <FilterSelect value={dc} onChange={setDc} all="All source DCs" options={dcs} />
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
          initialSorting={[{ id: "dailyProfit", desc: true }]}
          getRowId={(o) => `${o.item.id}-${o.quality}-${o.sourceWorldId}`}
          onRowClick={(o) => {
            openItem(o.item.id);
          }}
        />
      )}
    </>
  );
}

function FilterSelect(props: {
  value: string;
  onChange: (v: string) => void;
  all: string;
  options: string[];
}) {
  return (
    <Select value={props.value} onValueChange={props.onChange}>
      <SelectTrigger size="sm" className="w-44">
        <SelectValue />
      </SelectTrigger>
      <SelectContent className="max-h-72">
        <SelectItem value={ALL}>{props.all}</SelectItem>
        {props.options.map((o) => (
          <SelectItem key={o} value={o}>
            {o}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
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
