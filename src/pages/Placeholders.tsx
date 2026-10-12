// Pages whose features land in later milestones. They keep the empty-state
// pattern (one line + CTA) so navigation works end-to-end from M1.
import { Button } from "@/components/ui/button";
import { EmptyState, PageHeader } from "@/components/Page";
import { useNav } from "@/store/nav";

function Placeholder({ title, message }: { title: string; message: string }) {
  const setPage = useNav((s) => s.setPage);
  return (
    <>
      <PageHeader title={title} />
      <EmptyState
        message={message}
        action={
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              setPage("settings");
            }}
          >
            Review settings
          </Button>
        }
      />
    </>
  );
}

export const DashboardPage = () => (
  <Placeholder title="Dashboard" message="No scan yet. Scanning arrives in M3." />
);
export const ScanPage = () => <Placeholder title="Scan" message="Full scan arrives in M3." />;
export const WatchlistPage = () => (
  <Placeholder title="Watchlist" message="No starred items. Watchlist arrives in M5." />
);
export const SearchPage = () => <Placeholder title="Search" message="Item search arrives in M2." />;
export const HistoryPage = () => (
  <Placeholder title="History" message="No past scans. Scan history arrives in M3." />
);
