import { useEffect, useState } from "react";

/** Current time, refreshed every `everyMs` while `active` (render stays pure). */
export function useNow(active = true, everyMs = 1000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    const t = setInterval(() => {
      setNow(Date.now());
    }, everyMs);
    return () => {
      clearInterval(t);
    };
  }, [active, everyMs]);
  return now;
}
