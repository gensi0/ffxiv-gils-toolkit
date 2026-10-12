import { StarIcon } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/api";
import { useToggleWatch, useWatchlist } from "@/lib/queries";
import { cn } from "@/lib/utils";

export function WatchButton({ itemId }: { itemId: number }) {
  const list = useWatchlist();
  const toggle = useToggleWatch();
  const watched = list.data?.some((w) => w.itemId === itemId) ?? false;
  return (
    <Button
      variant="outline"
      size="sm"
      disabled={toggle.isPending || list.isPending}
      onClick={() => {
        toggle.mutate({ itemId, watched }, { onError: (e) => toast.error(errorMessage(e)) });
      }}
    >
      <StarIcon className={cn(watched && "fill-amber-400 text-amber-400")} />
      {watched ? "Watching" : "Add to Watchlist"}
    </Button>
  );
}
