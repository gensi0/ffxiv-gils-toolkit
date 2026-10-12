import {
  Bar,
  BarChart,
  CartesianGrid,
  Line,
  LineChart,
  Pie,
  PieChart,
  type PieLabelRenderProps,
  ResponsiveContainer,
  Tooltip as ChartTooltip,
  XAxis,
  YAxis,
} from "recharts";
import { InfoIcon } from "lucide-react";
import { EmptyState, PageHeader } from "@/components/Page";
import { Skeleton } from "@/components/ui/skeleton";
import { Badge } from "@/components/ui/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { ConfidenceBadge } from "@/components/market";
import { gil } from "@/lib/format";
import { useMarketStats } from "@/lib/queries";
import { errorMessage } from "@/lib/api";
import type { Confidence, RegionStat } from "@/lib/api";

// ---------- helpers ----------

const CONF_COLOR: Record<Confidence, string> = {
  HOT: "#f97316",
  OK: "#22c55e",
  COMPETITIVE: "#3b82f6",
  SATURATED: "#a855f7",
  SLOW: "#6b7280",
};

function ConfBadge({ label }: { label: string }) {
  return <ConfidenceBadge value={label as Confidence} />;
}

// ---------- info tooltip ----------

function InfoTip({ text }: { text: string }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <InfoIcon className="inline-block size-3 cursor-help text-muted-foreground/60 hover:text-muted-foreground" />
      </TooltipTrigger>
      <TooltipContent side="top" className="max-w-56 text-center leading-snug">
        {text}
      </TooltipContent>
    </Tooltip>
  );
}

// ---------- stat card ----------

function StatCard({
  label,
  value,
  sub,
  tip,
}: {
  label: string;
  value: string;
  sub?: string;
  tip?: string;
}) {
  return (
    <div className="rounded-lg border bg-card p-4">
      <p className="flex items-center gap-1 text-xs text-muted-foreground">
        {label}
        {tip && <InfoTip text={tip} />}
      </p>
      <p className="mt-1 text-2xl font-semibold tabular-nums">{value}</p>
      {sub && <p className="mt-0.5 text-xs text-muted-foreground">{sub}</p>}
    </div>
  );
}

// ---------- section header ----------

function Section({
  title,
  tip,
  children,
}: {
  title: string;
  tip?: string;
  children: React.ReactNode;
}) {
  return (
    <section className="space-y-3">
      <h2 className="flex items-center gap-1.5 text-sm font-semibold tracking-tight text-muted-foreground uppercase">
        {title}
        {tip && <InfoTip text={tip} />}
      </h2>
      {children}
    </section>
  );
}

// ---------- main page ----------

export function StatsPage() {
  const { data, isPending, error } = useMarketStats();

  if (isPending) {
    return (
      <>
        <PageHeader title="Stats" />
        <div className="space-y-4 p-4">
          {Array.from({ length: 6 }).map((_, i) => (
            <Skeleton key={i} className="h-32 w-full" />
          ))}
        </div>
      </>
    );
  }

  if (error) {
    return (
      <>
        <PageHeader title="Stats" />
        <EmptyState message={errorMessage(error)} />
      </>
    );
  }

  const hasHistory = data.scanHistory.length > 0;

  return (
    <>
      <PageHeader title="Stats" />
      <div className="space-y-8 overflow-y-auto p-4">
        {/* ── Overview ── */}
        <Section title="Overview">
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <StatCard
              label="Total scans"
              value={data.totalScans.toString()}
              tip="Nombre de scans complétés depuis le premier lancement."
            />
            <StatCard
              label="Total opportunities"
              value={data.totalOpportunities.toLocaleString()}
              sub={`~${data.avgOppsPerScan.toFixed(1)} per scan`}
              tip="Nombre total d'opportunités d'arbitrage trouvées sur tous tes scans. Une opportunité = un item moins cher sur un autre monde que chez toi."
            />
            <StatCard
              label="Best scan (gil/day)"
              value={gil(data.bestScanDailyProfit)}
              sub="single scan total"
              tip="Meilleur scan en valeur totale : somme des profits journaliers estimés de toutes les opportunités trouvées dans ce scan."
            />
            <StatCard
              label="Unique worlds sourced"
              value={data.worldRankings.length.toString()}
              tip="Nombre de mondes distincts qui ont produit au moins une opportunité d'achat sur l'ensemble de tes scans."
            />
          </div>
        </Section>

        {/* ── Scan history ── */}
        {hasHistory && (
          <Section
            title="Scan history — opportunities over time"
            tip="Évolution de tes scans dans le temps. Bleu = nb d'opportunités trouvées, orange = profit journalier total estimé (en milliers de gils)."
          >
            <div className="rounded-lg border bg-card p-3">
              <ResponsiveContainer width="100%" height={160}>
                <LineChart
                  data={data.scanHistory.map((p) => ({
                    date: new Date(p.startedAt).toLocaleDateString("fr", {
                      month: "2-digit",
                      day: "2-digit",
                    }),
                    opps: p.opportunitiesFound,
                    profit: Math.round(p.totalDailyProfit / 1000),
                  }))}
                >
                  <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border))" />
                  <XAxis
                    dataKey="date"
                    tick={{ fontSize: 10, fill: "hsl(var(--muted-foreground))" }}
                  />
                  <YAxis
                    yAxisId="opps"
                    tick={{ fontSize: 10, fill: "hsl(var(--muted-foreground))" }}
                  />
                  <YAxis
                    yAxisId="profit"
                    orientation="right"
                    tickFormatter={(v: number) => `${v}k`}
                    tick={{ fontSize: 10, fill: "hsl(var(--muted-foreground))" }}
                  />
                  <ChartTooltip
                    contentStyle={{
                      backgroundColor: "hsl(var(--card))",
                      border: "1px solid hsl(var(--border))",
                      borderRadius: "6px",
                      fontSize: 11,
                    }}
                    formatter={(value: unknown, name: unknown) =>
                      name === "profit"
                        ? [`${value as number}k g/day`, "Daily profit"]
                        : [`${value as number}`, "Opps"]
                    }
                  />
                  <Line
                    yAxisId="opps"
                    type="monotone"
                    dataKey="opps"
                    stroke="#3b82f6"
                    strokeWidth={1.5}
                    dot={false}
                  />
                  <Line
                    yAxisId="profit"
                    type="monotone"
                    dataKey="profit"
                    stroke="#f97316"
                    strokeWidth={1.5}
                    dot={false}
                  />
                </LineChart>
              </ResponsiveContainer>
              <p className="mt-1 text-center text-[10px] text-muted-foreground">
                Blue = opportunities · Orange = total gil/day (k)
              </p>
            </div>
          </Section>
        )}

        {/* ── World rankings ── */}
        {data.worldRankings.length > 0 && (
          <Section
            title="Best source worlds (by total gil/day across all scans)"
            tip="Classement des mondes sources par profit journalier cumulé sur tous tes scans. Un monde en tête = il a régulièrement des prix bas sur des items que tu peux revendre chez toi."
          >
            <div className="rounded-lg border bg-card p-3">
              <ResponsiveContainer
                width="100%"
                height={Math.max(160, data.worldRankings.slice(0, 15).length * 24)}
              >
                <BarChart
                  layout="vertical"
                  data={data.worldRankings.slice(0, 15).map((w) => ({
                    name: `${w.worldName} (${w.dc})`,
                    profit: Math.round(w.totalDailyProfit / 1000),
                    gap: Math.round(w.avgGapPct),
                    conf: w.bestConfidence,
                    fill: CONF_COLOR[w.bestConfidence],
                  }))}
                  margin={{ left: 100, right: 40, top: 4, bottom: 4 }}
                >
                  <CartesianGrid
                    horizontal={false}
                    strokeDasharray="3 3"
                    stroke="hsl(var(--border))"
                  />
                  <XAxis
                    type="number"
                    tickFormatter={(v: number) => `${v}k`}
                    tick={{ fontSize: 10, fill: "hsl(var(--muted-foreground))" }}
                  />
                  <YAxis
                    type="category"
                    dataKey="name"
                    width={96}
                    tick={{ fontSize: 10, fill: "hsl(var(--muted-foreground))" }}
                  />
                  <ChartTooltip
                    contentStyle={{
                      backgroundColor: "hsl(var(--card))",
                      border: "1px solid hsl(var(--border))",
                      borderRadius: "6px",
                      fontSize: 11,
                    }}
                    formatter={(v: unknown, name: unknown) =>
                      name === "profit"
                        ? [`${String(v)}k gil/day`, "Total profit"]
                        : [`${String(v)}%`, "Avg gap"]
                    }
                  />
                  <Bar dataKey="profit" radius={[0, 3, 3, 0]} fillOpacity={0.8} />
                </BarChart>
              </ResponsiveContainer>
            </div>

            {/* World table */}
            <div className="rounded-lg border">
              <table className="w-full text-xs">
                <thead>
                  <tr className="border-b bg-muted/30 text-muted-foreground">
                    <th className="px-3 py-2 text-left font-medium">World</th>
                    <th className="px-3 py-2 text-left font-medium">DC</th>
                    <th className="px-3 py-2 text-right font-medium">
                      <span className="inline-flex items-center justify-end gap-1">
                        Opps
                        <InfoTip text="Nombre total d'opportunités sourcées depuis ce monde sur l'ensemble des scans." />
                      </span>
                    </th>
                    <th className="px-3 py-2 text-right font-medium">
                      <span className="inline-flex items-center justify-end gap-1">
                        Avg gap
                        <InfoTip text="Écart moyen entre le prix d'achat sur ce monde et le prix médian chez toi, exprimé en % du prix maison. +50% = tu achètes en moyenne 50% moins cher que le médian chez toi." />
                      </span>
                    </th>
                    <th className="px-3 py-2 text-right font-medium">
                      <span className="inline-flex items-center justify-end gap-1">
                        Gil/day
                        <InfoTip text="Somme des profits journaliers estimés pour toutes les opportunités de ce monde, cumulées sur tous les scans." />
                      </span>
                    </th>
                    <th className="px-3 py-2 text-left font-medium">
                      <span className="inline-flex items-center gap-1">
                        Best
                        <InfoTip text="Meilleur niveau de confiance observé : HOT = demande très forte, OK = demande correcte, COMPETITIVE = marché disputé, SATURATED = offre > demande, SLOW = vente lente." />
                      </span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {data.worldRankings.slice(0, 15).map((w) => (
                    <tr key={w.worldId} className="border-b last:border-0 hover:bg-muted/20">
                      <td className="px-3 py-1.5 font-medium">{w.worldName}</td>
                      <td className="px-3 py-1.5 text-muted-foreground">{w.dc}</td>
                      <td className="px-3 py-1.5 text-right tabular-nums">{w.totalOpps}</td>
                      <td className="px-3 py-1.5 text-right text-emerald-500 tabular-nums">
                        +{w.avgGapPct.toFixed(0)}%
                      </td>
                      <td className="px-3 py-1.5 text-right tabular-nums">
                        {gil(w.totalDailyProfit)}
                      </td>
                      <td className="px-3 py-1.5">
                        <ConfBadge label={w.bestConfidence} />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Section>
        )}

        {/* ── DC rankings ── */}
        {data.dcRankings.length > 0 && (
          <Section
            title="Data center comparison"
            tip="Comparaison des datacenters par profit total disponible. Utile pour savoir sur quel DC concentrer tes trips."
          >
            <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
              {data.dcRankings.map((dc) => (
                <div key={dc.dc} className="space-y-1 rounded-lg border bg-card p-3">
                  <div className="flex items-center justify-between">
                    <span className="font-semibold">{dc.dc}</span>
                    <Badge variant="outline" className="text-[10px]">
                      {dc.worldCount} worlds
                    </Badge>
                  </div>
                  <p className="text-xl font-semibold text-amber-500 tabular-nums">
                    {gil(dc.totalDailyProfit)}
                    <span className="text-xs font-normal text-muted-foreground"> gil/day</span>
                  </p>
                  <div className="flex gap-3 text-xs text-muted-foreground">
                    <span>{dc.totalOpps.toLocaleString()} opps</span>
                    <span className="text-emerald-500">+{dc.avgGapPct.toFixed(0)}% avg gap</span>
                  </div>
                </div>
              ))}
            </div>
          </Section>
        )}

        {/* ── Region rankings ── */}
        {data.regionRankings.length > 0 && (
          <Section
            title="Region comparison"
            tip="Vue macro par région de jeu (Europe, Japan, North-America, Oceania). Utile si tu actives les trips cross-région dans tes settings."
          >
            <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
              {data.regionRankings.map((r: RegionStat) => (
                <div key={r.region} className="space-y-1 rounded-lg border bg-card p-3">
                  <div className="flex items-center justify-between">
                    <span className="font-semibold">{r.region}</span>
                    <div className="flex gap-1">
                      <Badge variant="outline" className="text-[10px]">
                        {r.dcCount} DC
                      </Badge>
                      <Badge variant="outline" className="text-[10px]">
                        {r.worldCount} W
                      </Badge>
                    </div>
                  </div>
                  <p className="text-xl font-semibold text-amber-500 tabular-nums">
                    {gil(r.totalDailyProfit)}
                    <span className="text-xs font-normal text-muted-foreground"> gil/day</span>
                  </p>
                  <div className="flex gap-3 text-xs text-muted-foreground">
                    <span>{r.totalOpps.toLocaleString()} opps</span>
                    <span className="text-emerald-500">+{r.avgGapPct.toFixed(0)}% avg gap</span>
                  </div>
                </div>
              ))}
            </div>
          </Section>
        )}

        {/* ── Confidence + ratio distributions ── */}
        <div className="grid gap-6 md:grid-cols-2">
          {data.confidenceDist.length > 0 && (
            <Section
              title="Opportunity confidence distribution"
              tip="Répartition de tes opportunités par niveau de confiance. HOT = item très demandé (vente < 1j), OK = demande correcte, COMPETITIVE = beaucoup de vendeurs, SATURATED = offre > demande, SLOW = vente incertaine."
            >
              <div className="rounded-lg border bg-card p-3">
                <ResponsiveContainer width="100%" height={180}>
                  <PieChart>
                    <Pie
                      data={data.confidenceDist.map((c) => ({ ...c, fill: CONF_COLOR[c.label] }))}
                      dataKey="count"
                      nameKey="label"
                      cx="50%"
                      cy="50%"
                      outerRadius={70}
                      label={(props: PieLabelRenderProps) =>
                        `${props.name ?? ""} ${((props.percent ?? 0) * 100).toFixed(0)}%`
                      }
                      labelLine={false}
                    ></Pie>
                    <ChartTooltip
                      contentStyle={{
                        backgroundColor: "hsl(var(--card))",
                        border: "1px solid hsl(var(--border))",
                        borderRadius: "6px",
                        fontSize: 11,
                      }}
                      formatter={(v: unknown) => [(v as number).toLocaleString(), "count"]}
                    />
                  </PieChart>
                </ResponsiveContainer>
              </div>
            </Section>
          )}

          {data.profitRatioDist.length > 0 && (
            <Section
              title="Price gap distribution (unit profit / home median)"
              tip="Distribution des opportunités selon l'écart de prix relatif. Une barre haute à droite = beaucoup d'items très bon marché ailleurs. Idéal pour identifier si ton marché est globalement cher ou compétitif."
            >
              <div className="rounded-lg border bg-card p-3">
                <ResponsiveContainer width="100%" height={180}>
                  <BarChart
                    data={data.profitRatioDist}
                    margin={{ top: 4, right: 8, bottom: 4, left: 8 }}
                  >
                    <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border))" />
                    <XAxis
                      dataKey="label"
                      tick={{ fontSize: 9, fill: "hsl(var(--muted-foreground))" }}
                    />
                    <YAxis tick={{ fontSize: 9, fill: "hsl(var(--muted-foreground))" }} />
                    <ChartTooltip
                      contentStyle={{
                        backgroundColor: "hsl(var(--card))",
                        border: "1px solid hsl(var(--border))",
                        borderRadius: "6px",
                        fontSize: 11,
                      }}
                      formatter={(v: unknown) => [(v as number).toLocaleString(), "opportunities"]}
                    />
                    <Bar dataKey="count" fill="#3b82f6" fillOpacity={0.8} radius={[3, 3, 0, 0]} />
                  </BarChart>
                </ResponsiveContainer>
              </div>
            </Section>
          )}
        </div>

        {/* ── Top items ── */}
        {data.topItems.length > 0 && (
          <Section
            title="Most recurring arbitrage items"
            tip="Items qui apparaissent le plus souvent comme opportunités d'arbitrage sur tes scans. Un item récurrent = bonne cible pour la watchlist."
          >
            <div className="rounded-lg border">
              <table className="w-full text-xs">
                <thead>
                  <tr className="border-b bg-muted/30 text-muted-foreground">
                    <th className="px-3 py-2 text-left font-medium">Item</th>
                    <th className="px-3 py-2 text-right font-medium">
                      <span className="inline-flex items-center justify-end gap-1">
                        Scans
                        <InfoTip text="Nombre de scans distincts où cet item est apparu comme opportunité rentable." />
                      </span>
                    </th>
                    <th className="px-3 py-2 text-right font-medium">
                      <span className="inline-flex items-center justify-end gap-1">
                        Avg profit/unit
                        <InfoTip text="Profit moyen par unité achetée = prix de vente estimé chez toi − coût d'achat − taxes vendeur." />
                      </span>
                    </th>
                    <th className="px-3 py-2 text-right font-medium">
                      <span className="inline-flex items-center justify-end gap-1">
                        Best gil/day
                        <InfoTip text="Meilleur profit journalier estimé observé pour cet item sur un seul scan (profit/unité × unités vendues par jour)." />
                      </span>
                    </th>
                    <th className="px-3 py-2 text-left font-medium">Confidence</th>
                  </tr>
                </thead>
                <tbody>
                  {data.topItems.map((item) => (
                    <tr key={item.itemId} className="border-b last:border-0 hover:bg-muted/20">
                      <td className="px-3 py-1.5 font-medium">
                        {item.name || (
                          <span className="text-muted-foreground">Item #{item.itemId}</span>
                        )}
                      </td>
                      <td className="px-3 py-1.5 text-right tabular-nums">{item.scanCount}</td>
                      <td className="px-3 py-1.5 text-right text-emerald-500 tabular-nums">
                        {gil(Math.round(item.avgUnitProfit))}
                      </td>
                      <td className="px-3 py-1.5 text-right tabular-nums">
                        {gil(item.maxDailyProfit)}
                      </td>
                      <td className="px-3 py-1.5">
                        <ConfBadge label={item.bestConfidence} />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Section>
        )}
      </div>
    </>
  );
}
