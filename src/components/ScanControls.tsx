import { useMutation } from "@tanstack/react-query";
import { Loader2Icon, PlayIcon, SquareIcon } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { api, errorMessage } from "@/lib/api";
import { duration, PHASE_LABEL } from "@/lib/format";
import { isRunning } from "@/lib/queries";
import { useLive } from "@/store/live";

/** Run / cancel button. */
export function ScanButton() {
  const scan = useLive((s) => s.scan);
  const running = isRunning(scan);
  const start = useMutation({
    mutationFn: api.startScan,
    onError: (e) => toast.error(errorMessage(e)),
  });
  if (running) {
    return (
      <Button variant="outline" size="sm" onClick={() => void api.cancelScan()}>
        <SquareIcon />
        Annuler le scan
      </Button>
    );
  }
  return (
    <Button
      size="sm"
      disabled={start.isPending}
      onClick={() => {
        start.mutate();
      }}
    >
      {start.isPending ? <Loader2Icon className="animate-spin" /> : <PlayIcon />}
      Lancer un scan
    </Button>
  );
}

/** Progress strip shown while a scan runs. */
export function ScanProgressBar() {
  const scan = useLive((s) => s.scan);
  if (!scan || !isRunning(scan)) return null;
  const pct = scan.total > 0 ? (scan.done / scan.total) * 100 : 0;
  return (
    <div className="flex items-center gap-3 border-b px-4 py-2 text-xs">
      <span className="w-36 shrink-0">{PHASE_LABEL[scan.phase]}</span>
      <Progress value={pct} className="h-1.5 flex-1" />
      <span className="w-40 shrink-0 text-right text-muted-foreground tabular-nums">
        {scan.done}/{scan.total}
        {scan.etaSecs != null && ` · ${duration(scan.etaSecs)} restantes`}
      </span>
      {scan.itemsFailed > 0 && (
        <span className="shrink-0 text-amber-600 dark:text-amber-400">
          {scan.itemsFailed} en échec
        </span>
      )}
    </div>
  );
}
