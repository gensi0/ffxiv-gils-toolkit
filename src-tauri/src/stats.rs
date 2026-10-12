//! Aggregated market statistics derived from historical scan data.
//!
//! All stats come from the local SQLite DB — no extra Universalis calls.
//! Uses `sqlx::query_as` (runtime-checked) to avoid the offline query
//! cache requirement of `sqlx::query!`.

use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::AppResult;

// ---------- output types ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketStats {
    pub total_scans: i64,
    pub total_opportunities: i64,
    pub best_scan_daily_profit: i64,
    pub avg_opps_per_scan: f64,
    pub world_rankings: Vec<WorldStat>,
    pub dc_rankings: Vec<DcStat>,
    pub region_rankings: Vec<RegionStat>,
    pub confidence_dist: Vec<ConfidenceBucket>,
    pub top_items: Vec<TopItem>,
    pub scan_history: Vec<ScanPoint>,
    pub profit_ratio_dist: Vec<RatioBucket>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldStat {
    pub world_id: i64,
    pub world_name: String,
    pub dc: String,
    pub total_opps: i64,
    pub total_daily_profit: i64,
    pub avg_unit_profit: f64,
    pub avg_gap_pct: f64,
    pub best_confidence: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DcStat {
    pub dc: String,
    pub world_count: i64,
    pub total_opps: i64,
    pub total_daily_profit: i64,
    pub avg_gap_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionStat {
    pub region: String,
    pub dc_count: i64,
    pub world_count: i64,
    pub total_opps: i64,
    pub total_daily_profit: i64,
    pub avg_gap_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfidenceBucket {
    pub label: String,
    pub count: i64,
    pub total_daily_profit: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopItem {
    pub item_id: i64,
    pub name: String,
    pub scan_count: i64,
    pub avg_unit_profit: f64,
    pub max_daily_profit: i64,
    pub best_confidence: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPoint {
    pub started_at: i64,
    pub opportunities_found: i64,
    pub total_daily_profit: i64,
    pub items_scanned: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RatioBucket {
    pub lower: f64,
    pub label: String,
    pub count: i64,
}

// ---------- internal row types ----------

#[derive(sqlx::FromRow)]
struct SummaryRow {
    total_scans: i64,
    total_opps: i64,
    avg_opps: f64,
}

#[derive(sqlx::FromRow)]
struct BestScanRow {
    total: i64,
}

#[derive(sqlx::FromRow)]
struct WorldRow {
    world_id: i64,
    world_name: String,
    dc: String,
    total_opps: i64,
    total_daily_profit: i64,
    avg_unit_profit: f64,
    avg_gap_pct: f64,
    conf_rank: i64,
}

#[derive(sqlx::FromRow)]
struct DcRow {
    dc: String,
    world_count: i64,
    total_opps: i64,
    total_daily_profit: i64,
    avg_gap_pct: f64,
}

#[derive(sqlx::FromRow)]
struct RegionRow {
    region: String,
    dc_count: i64,
    world_count: i64,
    total_opps: i64,
    total_daily_profit: i64,
    avg_gap_pct: f64,
}

#[derive(sqlx::FromRow)]
struct ConfRow {
    label: String,
    count: i64,
    total_daily_profit: i64,
}

#[derive(sqlx::FromRow)]
struct ItemRow {
    item_id: i64,
    name: String,
    scan_count: i64,
    avg_unit_profit: f64,
    max_daily_profit: i64,
    conf_rank: i64,
}

#[derive(sqlx::FromRow)]
struct HistoryRow {
    started_at: i64,
    opportunities_found: i64,
    total_daily_profit: i64,
    items_scanned: i64,
}

#[derive(sqlx::FromRow)]
struct RatioRow {
    b0: i64,
    b1: i64,
    b2: i64,
    b3: i64,
    b4: i64,
    b5: i64,
}

// ---------- helpers ----------

fn conf_label(rank: i64) -> &'static str {
    match rank {
        4 => "HOT",
        3 => "OK",
        2 => "COMPETITIVE",
        1 => "SLOW",
        _ => "SATURATED",
    }
}

// ---------- main query ----------

pub async fn market_stats(db: &SqlitePool) -> AppResult<MarketStats> {
    // Resolve the latest completed scan — everything else filters on it.
    #[derive(sqlx::FromRow)]
    struct ScanIdRow { scan_id: String }

    let latest: Option<ScanIdRow> = sqlx::query_as(
        "SELECT scan_id FROM scan_runs WHERE status = 'done' ORDER BY started_at DESC LIMIT 1",
    )
    .fetch_optional(db)
    .await?;

    let Some(latest) = latest else {
        // No completed scan yet — return empty stats.
        return Ok(MarketStats {
            total_scans: 0,
            total_opportunities: 0,
            best_scan_daily_profit: 0,
            avg_opps_per_scan: 0.0,
            world_rankings: vec![],
            dc_rankings: vec![],
            region_rankings: vec![],
            confidence_dist: vec![],
            top_items: vec![],
            scan_history: vec![],
            profit_ratio_dist: vec![],
        });
    };
    let scan_id = &latest.scan_id;

    // Total completed scans (context only — all rankings filter on scan_id).
    let summary: SummaryRow = sqlx::query_as(
        "SELECT COUNT(*) AS total_scans,
                COALESCE(SUM(opportunities_found), 0) AS total_opps,
                COALESCE(AVG(CASE WHEN opportunities_found > 0
                             THEN CAST(opportunities_found AS REAL) END), 0.0) AS avg_opps
         FROM scan_runs WHERE status = 'done'",
    )
    .fetch_one(db)
    .await?;

    // Last scan total daily profit (= best, since we only look at the last scan).
    let best_scan: Option<BestScanRow> = sqlx::query_as(
        "SELECT COALESCE(SUM(daily_profit), 0) AS total FROM opportunities WHERE scan_id = ?",
    )
    .bind(scan_id)
    .fetch_optional(db)
    .await?;
    let best_scan_daily_profit = best_scan.map(|r| r.total).unwrap_or(0);

    let world_rows: Vec<WorldRow> = sqlx::query_as(
        "SELECT o.source_world_id AS world_id,
                COALESCE(w.name, CAST(o.source_world_id AS TEXT)) AS world_name,
                COALESCE(w.dc_name, '') AS dc,
                COUNT(*) AS total_opps,
                COALESCE(SUM(o.daily_profit), 0) AS total_daily_profit,
                COALESCE(AVG(CAST(o.unit_profit AS REAL)), 0.0) AS avg_unit_profit,
                COALESCE(AVG(CASE WHEN o.home_median_price > 0
                         THEN CAST(o.unit_profit AS REAL) / o.home_median_price
                         ELSE NULL END), 0.0) AS avg_gap_pct,
                COALESCE(MAX(CASE o.confidence
                    WHEN 'HOT' THEN 4 WHEN 'OK' THEN 3
                    WHEN 'COMPETITIVE' THEN 2 WHEN 'SLOW' THEN 1
                    ELSE 0 END), 0) AS conf_rank
         FROM opportunities o
         LEFT JOIN worlds w ON w.world_id = o.source_world_id
         WHERE o.scan_id = ?
         GROUP BY o.source_world_id
         ORDER BY total_daily_profit DESC
         LIMIT 30",
    )
    .bind(scan_id)
    .fetch_all(db)
    .await?;

    let world_rankings: Vec<WorldStat> = world_rows
        .into_iter()
        .map(|r| WorldStat {
            world_id: r.world_id,
            world_name: r.world_name,
            dc: r.dc,
            total_opps: r.total_opps,
            total_daily_profit: r.total_daily_profit,
            avg_unit_profit: r.avg_unit_profit,
            avg_gap_pct: r.avg_gap_pct * 100.0,
            best_confidence: conf_label(r.conf_rank).to_string(),
        })
        .collect();

    let dc_rows: Vec<DcRow> = sqlx::query_as(
        "SELECT COALESCE(w.dc_name, 'Unknown') AS dc,
                COUNT(DISTINCT o.source_world_id) AS world_count,
                COUNT(*) AS total_opps,
                COALESCE(SUM(o.daily_profit), 0) AS total_daily_profit,
                COALESCE(AVG(CASE WHEN o.home_median_price > 0
                         THEN CAST(o.unit_profit AS REAL) / o.home_median_price
                         ELSE NULL END), 0.0) AS avg_gap_pct
         FROM opportunities o
         LEFT JOIN worlds w ON w.world_id = o.source_world_id
         WHERE o.scan_id = ?
         GROUP BY dc
         ORDER BY total_daily_profit DESC",
    )
    .bind(scan_id)
    .fetch_all(db)
    .await?;

    let dc_rankings: Vec<DcStat> = dc_rows
        .into_iter()
        .map(|r| DcStat {
            dc: r.dc,
            world_count: r.world_count,
            total_opps: r.total_opps,
            total_daily_profit: r.total_daily_profit,
            avg_gap_pct: r.avg_gap_pct * 100.0,
        })
        .collect();

    // --- region rankings ---
    let region_rows: Vec<RegionRow> = sqlx::query_as(
        "SELECT COALESCE(w.region, 'Unknown') AS region,
                COUNT(DISTINCT w.dc_name) AS dc_count,
                COUNT(DISTINCT o.source_world_id) AS world_count,
                COUNT(*) AS total_opps,
                COALESCE(SUM(o.daily_profit), 0) AS total_daily_profit,
                COALESCE(AVG(CASE WHEN o.home_median_price > 0
                         THEN CAST(o.unit_profit AS REAL) / o.home_median_price
                         ELSE NULL END), 0.0) AS avg_gap_pct
         FROM opportunities o
         LEFT JOIN worlds w ON w.world_id = o.source_world_id
         WHERE o.scan_id = ?
         GROUP BY region
         ORDER BY total_daily_profit DESC",
    )
    .bind(scan_id)
    .fetch_all(db)
    .await?;

    let region_rankings: Vec<RegionStat> = region_rows
        .into_iter()
        .map(|r| RegionStat {
            region: r.region,
            dc_count: r.dc_count,
            world_count: r.world_count,
            total_opps: r.total_opps,
            total_daily_profit: r.total_daily_profit,
            avg_gap_pct: r.avg_gap_pct * 100.0,
        })
        .collect();

    let conf_rows: Vec<ConfRow> = sqlx::query_as(
        "SELECT confidence AS label,
                COUNT(*) AS count,
                COALESCE(SUM(daily_profit), 0) AS total_daily_profit
         FROM opportunities
         WHERE scan_id = ?
         GROUP BY confidence
         ORDER BY count DESC",
    )
    .bind(scan_id)
    .fetch_all(db)
    .await?;

    let confidence_dist: Vec<ConfidenceBucket> = conf_rows
        .into_iter()
        .map(|r| ConfidenceBucket {
            label: r.label,
            count: r.count,
            total_daily_profit: r.total_daily_profit,
        })
        .collect();

    let item_rows: Vec<ItemRow> = sqlx::query_as(
        "SELECT o.item_id,
                COALESCE(xi.name_en, '') AS name,
                COUNT(DISTINCT o.scan_id) AS scan_count,
                COALESCE(AVG(CAST(o.unit_profit AS REAL)), 0.0) AS avg_unit_profit,
                COALESCE(MAX(o.daily_profit), 0) AS max_daily_profit,
                COALESCE(MAX(CASE o.confidence
                    WHEN 'HOT' THEN 4 WHEN 'OK' THEN 3
                    WHEN 'COMPETITIVE' THEN 2 WHEN 'SLOW' THEN 1
                    ELSE 0 END), 0) AS conf_rank
         FROM opportunities o
         LEFT JOIN items xi ON xi.item_id = o.item_id
         WHERE o.scan_id = ?
         GROUP BY o.item_id
         ORDER BY avg_unit_profit DESC
         LIMIT 20",
    )
    .bind(scan_id)
    .fetch_all(db)
    .await?;

    let top_items: Vec<TopItem> = item_rows
        .into_iter()
        .map(|r| TopItem {
            item_id: r.item_id,
            name: r.name,
            scan_count: r.scan_count,
            avg_unit_profit: r.avg_unit_profit,
            max_daily_profit: r.max_daily_profit,
            best_confidence: conf_label(r.conf_rank).to_string(),
        })
        .collect();

    let history_rows: Vec<HistoryRow> = sqlx::query_as(
        "SELECT r.started_at,
                COALESCE(r.opportunities_found, 0) AS opportunities_found,
                COALESCE(SUM(o.daily_profit), 0) AS total_daily_profit,
                COALESCE(r.items_scanned, 0) AS items_scanned
         FROM scan_runs r
         LEFT JOIN opportunities o ON o.scan_id = r.scan_id
         WHERE r.status = 'done'
         GROUP BY r.scan_id
         ORDER BY r.started_at ASC
         LIMIT 50",
    )
    .fetch_all(db)
    .await?;

    let scan_history: Vec<ScanPoint> = history_rows
        .into_iter()
        .map(|r| ScanPoint {
            started_at: r.started_at * 1000,
            opportunities_found: r.opportunities_found,
            total_daily_profit: r.total_daily_profit,
            items_scanned: r.items_scanned,
        })
        .collect();

    let ratio: RatioRow = sqlx::query_as(
        "SELECT
           COALESCE(SUM(CASE WHEN home_median_price > 0
               AND CAST(unit_profit AS REAL)/home_median_price < 0.10 THEN 1 ELSE 0 END), 0) AS b0,
           COALESCE(SUM(CASE WHEN home_median_price > 0
               AND CAST(unit_profit AS REAL)/home_median_price BETWEEN 0.10 AND 0.249 THEN 1 ELSE 0 END), 0) AS b1,
           COALESCE(SUM(CASE WHEN home_median_price > 0
               AND CAST(unit_profit AS REAL)/home_median_price BETWEEN 0.25 AND 0.499 THEN 1 ELSE 0 END), 0) AS b2,
           COALESCE(SUM(CASE WHEN home_median_price > 0
               AND CAST(unit_profit AS REAL)/home_median_price BETWEEN 0.50 AND 0.999 THEN 1 ELSE 0 END), 0) AS b3,
           COALESCE(SUM(CASE WHEN home_median_price > 0
               AND CAST(unit_profit AS REAL)/home_median_price BETWEEN 1.00 AND 1.999 THEN 1 ELSE 0 END), 0) AS b4,
           COALESCE(SUM(CASE WHEN home_median_price > 0
               AND CAST(unit_profit AS REAL)/home_median_price >= 2.00 THEN 1 ELSE 0 END), 0) AS b5
         FROM opportunities WHERE scan_id = ?",
    )
    .bind(scan_id)
    .fetch_one(db)
    .await?;

    let profit_ratio_dist = vec![
        RatioBucket { lower: 0.0,  label: "< 10%".into(),    count: ratio.b0 },
        RatioBucket { lower: 0.10, label: "10-25%".into(),   count: ratio.b1 },
        RatioBucket { lower: 0.25, label: "25-50%".into(),   count: ratio.b2 },
        RatioBucket { lower: 0.50, label: "50-100%".into(),  count: ratio.b3 },
        RatioBucket { lower: 1.00, label: "100-200%".into(), count: ratio.b4 },
        RatioBucket { lower: 2.00, label: "> 200%".into(),   count: ratio.b5 },
    ];

    Ok(MarketStats {
        total_scans: summary.total_scans,
        total_opportunities: summary.total_opps,
        best_scan_daily_profit,
        avg_opps_per_scan: summary.avg_opps,
        world_rankings,
        dc_rankings,
        region_rankings,
        confidence_dist,
        top_items,
        scan_history,
        profit_ratio_dist,
    })
}
