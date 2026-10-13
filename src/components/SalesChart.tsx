import { useMemo, useState } from "react";
import {
  CartesianGrid,
  ComposedChart,
  Line,
  ResponsiveContainer,
  Scatter,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { Button } from "@/components/ui/button";
import type { SalePoint } from "@/lib/api";
import { gil, gilShort } from "@/lib/format";
import { useSettings } from "@/lib/queries";

const DAY_MS = 86_400_000;
const WINDOW_DAYS = 28;

/** Categorical slots 1-2, validated (all-pairs, CVD + contrast) on both app surfaces. */
const SERIES = {
  light: { nq: "#2a78d6", hq: "#eb6834", grid: "#e5e5e5", text: "#737373", surface: "#ffffff" },
  dark: { nq: "#3987e5", hq: "#d95926", grid: "#262626", text: "#a3a3a3", surface: "#0a0a0a" },
} as const;

interface Dot {
  t: number;
  price: number;
  quantity: number;
  hq: boolean;
}
interface DailyPoint {
  t: number;
  median: number;
}

/** Quantity-weighted median per day, plotted at noon. */
function dailyMedians(points: SalePoint[]): DailyPoint[] {
  const byDay = new Map<number, SalePoint[]>();
  for (const p of points) {
    const day = Math.floor(p.t / DAY_MS);
    const list = byDay.get(day);
    if (list) list.push(p);
    else byDay.set(day, [p]);
  }
  return [...byDay.entries()]
    .sort(([a], [b]) => a - b)
    .map(([day, list]) => {
      const sorted = [...list].sort((a, b) => a.price - b.price);
      const half = sorted.reduce((s, p) => s + p.quantity, 0) / 2;
      let acc = 0;
      const median = sorted.find((p) => (acc += p.quantity) >= half)?.price ?? 0;
      return { t: day * DAY_MS + DAY_MS / 2, median };
    });
}

/** Upper bound that ignores the top 2% of prices, so one absurd sale cannot flatten the chart. */
function priceCeiling(points: SalePoint[]): number {
  if (points.length === 0) return 1;
  const prices = points.map((p) => p.price).sort((a, b) => a - b);
  const p98 = prices[Math.min(prices.length - 1, Math.floor(prices.length * 0.98))] ?? 1;
  return Math.max(1, Math.ceil(p98 * 1.15));
}

function SaleTooltip({
  active,
  payload,
}: {
  active?: boolean;
  payload?: readonly { payload?: unknown }[];
}) {
  const dot = payload?.[0]?.payload as Partial<Dot & DailyPoint> | undefined;
  if (!active || !dot?.t) return null;
  const isSale = dot.price !== undefined;
  return (
    <div className="rounded-md border bg-popover px-2 py-1.5 text-xs text-popover-foreground shadow-md">
      <div className="text-muted-foreground">
        {new Date(dot.t).toLocaleString("fr-FR", {
          dateStyle: "medium",
          timeStyle: isSale ? "short" : undefined,
        })}
      </div>
      {isSale ? (
        <div className="tabular-nums">
          {dot.hq ? "HQ" : "NQ"} · {gil(dot.quantity)} × {gil(dot.price)} gils
        </div>
      ) : (
        <div className="tabular-nums">Médiane du jour {gil(dot.median)} gils</div>
      )}
    </div>
  );
}

export function SalesChart({
  sales,
  canBeHq,
  now,
}: {
  sales: SalePoint[];
  canBeHq: boolean;
  now: number;
}) {
  const theme = useSettings().data?.theme ?? "dark";
  const c = SERIES[theme];
  const [showTable, setShowTable] = useState(false);

  const data = useMemo(() => {
    const nq = sales.filter((s) => !s.hq);
    const hq = sales.filter((s) => s.hq);
    const ceiling = priceCeiling(sales);
    return {
      nq,
      hq,
      nqDaily: dailyMedians(nq),
      hqDaily: dailyMedians(hq),
      ceiling,
      hidden: sales.filter((s) => s.price > ceiling).length,
    };
  }, [sales]);

  if (sales.length === 0) {
    return (
      <p className="text-xs text-muted-foreground">
        Aucune vente sur le serveur principal ces 28 derniers jours.
      </p>
    );
  }
  const showHq = canBeHq && data.hq.length > 0;
  const domain: [number, number] = [now - WINDOW_DAYS * DAY_MS, now];

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-4 text-[11px] text-muted-foreground">
        <LegendKey color={c.nq} label={`NQ · ${data.nq.length} ventes`} />
        {showHq && <LegendKey color={c.hq} label={`HQ · ${data.hq.length} ventes`} />}
        <span>Points : ventes · lignes : médiane du jour.</span>
        {data.hidden > 0 && <span>{data.hidden} ventes aberrantes au-dessus du graphique.</span>}
        <Button
          variant="ghost"
          size="xs"
          className="ml-auto"
          onClick={() => {
            setShowTable((v) => !v);
          }}
        >
          {showTable ? "Masquer le tableau" : "Afficher le tableau"}
        </Button>
      </div>
      <div className="h-56 w-full">
        <ResponsiveContainer>
          <ComposedChart margin={{ top: 8, right: 12, bottom: 0, left: 0 }}>
            <CartesianGrid stroke={c.grid} strokeWidth={1} vertical={false} />
            <XAxis
              type="number"
              dataKey="t"
              domain={domain}
              scale="time"
              ticks={Array.from(
                { length: 5 },
                (_, i) => domain[0] + (i * (domain[1] - domain[0])) / 4,
              )}
              tickFormatter={(t: number) =>
                new Date(t).toLocaleDateString("fr-FR", { day: "numeric", month: "short" })
              }
              tick={{ fill: c.text, fontSize: 11 }}
              stroke={c.grid}
              allowDataOverflow
            />
            <YAxis
              type="number"
              domain={[0, data.ceiling]}
              allowDataOverflow
              tickFormatter={(v: number) => gilShort(v)}
              tick={{ fill: c.text, fontSize: 11 }}
              stroke={c.grid}
              width={52}
            />
            <Tooltip
              content={(p) => <SaleTooltip active={p.active} payload={p.payload} />}
              cursor={{ stroke: c.grid, strokeWidth: 1 }}
            />
            <Scatter
              data={data.nq}
              dataKey="price"
              fill={c.nq}
              fillOpacity={0.55}
              stroke={c.surface}
              strokeWidth={2}
              shape="circle"
              isAnimationActive={false}
            />
            {showHq && (
              <Scatter
                data={data.hq}
                dataKey="price"
                fill={c.hq}
                fillOpacity={0.55}
                stroke={c.surface}
                strokeWidth={2}
                shape="circle"
                isAnimationActive={false}
              />
            )}
            <Line
              data={data.nqDaily}
              dataKey="median"
              stroke={c.nq}
              strokeWidth={2}
              dot={false}
              activeDot={{ r: 4, stroke: c.surface, strokeWidth: 2 }}
              isAnimationActive={false}
            />
            {showHq && (
              <Line
                data={data.hqDaily}
                dataKey="median"
                stroke={c.hq}
                strokeWidth={2}
                dot={false}
                activeDot={{ r: 4, stroke: c.surface, strokeWidth: 2 }}
                isAnimationActive={false}
              />
            )}
          </ComposedChart>
        </ResponsiveContainer>
      </div>
      {showTable && <SalesTable sales={sales} />}
    </div>
  );
}

function LegendKey({ color, label }: { color: string; label: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <span className="h-0.5 w-3 rounded-full" style={{ background: color }} />
      {label}
    </span>
  );
}

function SalesTable({ sales }: { sales: SalePoint[] }) {
  const rows = [...sales].reverse().slice(0, 200);
  return (
    <div className="max-h-56 overflow-y-auto rounded-md border">
      <table className="w-full text-xs">
        <thead className="sticky top-0 bg-background text-muted-foreground">
          <tr className="border-b">
            <th className="h-7 px-2 text-left font-medium">Date</th>
            <th className="px-2 text-left font-medium">Q</th>
            <th className="px-2 text-right font-medium">Unités</th>
            <th className="px-2 text-right font-medium">Prix / unité</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((s, i) => (
            <tr key={`${s.t}-${i}`} className="border-b border-border/50">
              <td className="h-6 px-2">{new Date(s.t).toLocaleString("fr-FR")}</td>
              <td className="px-2">{s.hq ? "HQ" : "NQ"}</td>
              <td className="px-2 text-right tabular-nums">{gil(s.quantity)}</td>
              <td className="px-2 text-right tabular-nums">{gil(s.price)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {sales.length > rows.length && (
        <p className="px-2 py-1 text-[11px] text-muted-foreground">
          {rows.length} ventes les plus récentes sur {sales.length}.
        </p>
      )}
    </div>
  );
}
