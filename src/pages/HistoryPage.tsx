import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { DataTable } from "@/components/DataTable";
import { columnHelper } from "@/lib/table";
import { EmptyState, PageHeader } from "@/components/Page";
import { ScanButton } from "@/components/ScanControls";
import { errorMessage, type ScanRun } from "@/lib/api";
import { duration, gil } from "@/lib/format";
import { useScans } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { useNav } from "@/store/nav";

const STATUS_CLASS: Record<ScanRun["status"], string> = {
  done: "text-foreground",
  running: "text-sky-600 dark:text-sky-400",
  failed: "text-destructive",
  cancelled: "text-muted-foreground",
};

const c = columnHelper<ScanRun>();
const columns = [
  c.accessor("startedAt", {
    header: "Started",
    cell: (v) => new Date(v.getValue()).toLocaleString(),
  }),
  c.accessor("status", {
    header: "Status",
    cell: (v) => (
      <span
        className={cn("capitalize", STATUS_CLASS[v.getValue()])}
        title={v.row.original.error ?? undefined}
      >
        {v.getValue()}
      </span>
    ),
  }),
  c.accessor((r) => (r.completedAt ? (r.completedAt - r.startedAt) / 1000 : undefined), {
    id: "duration",
    header: "Duration",
    cell: (v) => {
      const secs = v.getValue();
      return secs === undefined ? "—" : duration(secs);
    },
    meta: { numeric: true },
    sortUndefined: "last",
  }),
  c.accessor("itemsScanned", {
    header: "Items",
    cell: (v) => gil(v.getValue()),
    meta: { numeric: true },
  }),
  c.accessor("itemsFailed", {
    header: "Failed",
    cell: (v) => gil(v.getValue()),
    meta: { numeric: true },
  }),
  c.accessor("opportunitiesFound", {
    header: "Opportunities",
    cell: (v) => gil(v.getValue()),
    meta: { numeric: true },
  }),
];

export function HistoryPage() {
  const scans = useScans();
  const openScan = useNav((s) => s.openScan);
  return (
    <>
      <PageHeader title="History" />
      {scans.isError ? (
        <div className="flex items-center gap-3 p-4 text-xs text-destructive">
          {errorMessage(scans.error)}
          <Button variant="outline" size="sm" onClick={() => void scans.refetch()}>
            Retry
          </Button>
        </div>
      ) : !scans.data ? (
        <div className="flex flex-col gap-1 p-4">
          {Array.from({ length: 6 }, (_, i) => (
            <Skeleton key={i} className="h-8" />
          ))}
        </div>
      ) : scans.data.length === 0 ? (
        <EmptyState message="No past scans." action={<ScanButton />} />
      ) : (
        <DataTable
          columns={columns}
          data={scans.data}
          initialSorting={[{ id: "startedAt", desc: true }]}
          getRowId={(r) => r.scanId}
          onRowClick={(r) => {
            if (r.status === "done") openScan(r.scanId);
          }}
        />
      )}
    </>
  );
}
