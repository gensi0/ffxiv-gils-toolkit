// Pages whose features land in later milestones. They keep the empty-state
// pattern (one line + CTA) so navigation works end-to-end.
import { Button } from "@/components/ui/button";
import { EmptyState, PageHeader } from "@/components/Page";
import { useNav } from "@/store/nav";

export function WatchlistPage() {
  const setPage = useNav((s) => s.setPage);
  return (
    <>
      <PageHeader title="Watchlist" />
      <EmptyState
        message="No starred items. Live watchlist arrives in M5."
        action={
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              setPage("search");
            }}
          >
            Search items
          </Button>
        }
      />
    </>
  );
}
