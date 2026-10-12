//! Aggregated market statistics derived from historical scan data.
//!
//! All stats come from the local SQLite DB — no extra Universalis calls.
//! Fast to load, works offline, reflects the user's actual scan history.

use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::AppResult;

// ---------- output types ----------

/// Top-level stats payload sent to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketStats {
    /// How many completed scans have been run.
    pub total_scans: i64,
    /// Total opportunities found across all scans.
    pub total_opportunities: i64,
    /// Best single-scan daily profit total (gil/day).
    pub best_scan_daily_profit: i64,
    /// Average opportunities per completed scan.
    pub avg_opps_per_scan: f64,

    /// Per-world ranking: best source worlds for arbitrage.
    pub world_rankings: Vec<WorldStat>,
    /// Per-DC summary.
    pub dc_rankings: Vec<DcStat>,
    /// Breakdown of opportunity confidence labels.
    pub confidence_dist: Vec<ConfidenceBucket>,
    /// Most frequently appearing items across all scans.
    pub top_items: Vec<TopItem>,
    /// Scan history over time (one point per completed scan).
    pub scan_history: Vec<ScanPoint>,
    /// Distribution of unit-profit-to-home-median ratios (% buckets).
    pub profit_ratio_dist: Vec<RatioBucket>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldStat {
    pub world_id: i64,
    pub world_name: String,
    pub dc: String,
    /// Total opportunities sourced from this world across all scans.
    pub total_opps: i64,
    /// Sum of daily_profit for all opportunities on this world.
    pub total_daily_profit: i64,
    /// Average unit_profit across all opportunities on this world.
    pub avg_unit_profit: f64,
    /// Average price gap ratio (unit_profit / home_median_price).
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
pub struct ConfidenceBucket {
    pub label: String,
    pub count: i64,
    pub total_daily_profit: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopItem {
    pub item_id: i64,
    /// Best known name from the xivapi cache; empty string if not cached.
    pub name: String,
    /// How many scans this item appeared in.
    pub scan_count: i64,
    pub avg_unit_profit: f64,
    pub max_daily_profit: i64,
    pub best_confidence: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPoint {
    /// Unix ms.
    pub started_at: i64,
    pub opportunities_found: i64,
    pub total_daily_profit: i64,
    pub items_scanned: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RatioBucket {
    /// Lower bound (e.g. 0.0 → "0–10%").
    pub lower: f64,
    pub label: String,
    pub count: i64,
}

// ---------- query ----------

pub async fn market_stats(db: &SqlitePool) -> AppResult<MarketStats> {
    // --- scan summary ---
    let summary = sqlx::query!(
        "SELECT COUNT(*) AS total_scans,
                COALESCE(SUM(opportunities_found), 0) AS total_opps,
                COALESCE(AVG(CASE WHEN opportunities_found > 0
                             THEN opportunities_found END), 0.0) AS avg_opps
         FROM scan_runs WHERE status = 'done'"
    )
    .fetch_one(db)
    .await?;

    let total_scans = summary.total_scans;
    let total_opportunities = summary.total_opps.unwrap_or(0);
    let avg_opps_per_scan = summary.avg_opps.unwrap_or(0.0);

    // Best single-scan total daily profit
    let best_scan = sqlx::query!(
        "SELECT COALESCE(SUM(o.daily_profit), 0) AS total
         FROM opportunities o
         JOIN scan_runs r ON r.scan_id = o.scan_id
         WHERE r.status = 'done'
         GROUP BY o.scan_id
         ORDER BY total DESC LIMIT 1"
    )
    .fetch_optional(db)
    .await?;
    let best_scan_daily_profit = best_scan.and_then(|r| r.total).unwrap_or(0);

    // --- world rankings ---
    // Join worlds table for the name; fall back to world_id string if not found.
    let world_rows = sqlx::query!(
        "SELECT o.source_world_id AS world_id,
                COALESCE(w.name, CAST(o.source_world_id AS TEXT)) AS world_name,
                COALESCE(w.dc_name, '') AS dc,
                COUNT(*) AS total_opps,
                SUM(o.daily_profit) AS total_daily_profit,
                AVG(CAST(o.unit_profit AS REAL)) AS avg_unit_profit,
                AVG(CASE WHEN o.home_median_price > 0
                         THEN CAST(o.unit_profit AS REAL) / o.home_median_price
                         ELSE NULL END) AS avg_gap_pct,
                MAX(CASE o.confidence
                    WHEN 'HOT' THEN 4 WHEN 'OK' THEN 3
                    WHEN 'COMPETITIVE' THEN 2 WHEN 'SLOW' THEN 1
                    ELSE 0 END) AS conf_rank
         FROM opportunities o
         LEFT JOIN worlds w ON w.world_id = o.source_world_id
         GROUP BY o.source_world_id
         ORDER BY total_daily_profit DESC
         LIMIT 30"
    )
    .fetch_all(db)
    .await?;

    let conf_label = |rank: i64| match rank {
        4 => "HOT",
        3 => "OK",
        2 => "COMPETITIVE",
        1 => "SLOW",
        _ => "SATURATED",
    };

    let world_rankings: Vec<WorldStat> = world_rows
        .into_iter()
        .map(|r| WorldStat {
            world_id: r.world_id,
            world_name: r.world_name.unwrap_or_default(),
            dc: r.dc.unwrap_or_default(),
            total_opps: r.total_opps,
            total_daily_profit: r.total_daily_profit.unwrap_or(0),
            avg_unit_profit: r.avg_unit_profit.unwrap_or(0.0),
            avg_gap_pct: r.avg_gap_pct.unwrap_or(0.0) * 100.0,
            best_confidence: conf_label(r.conf_rank.unwrap_or(0)).to_string(),
        })
        .collect();

    // --- DC rankings ---
    let dc_rows = sqlx::query!(
        "SELECT COALESCE(w.dc_name, 'Unknown') AS dc,
                COUNT(DISTINCT o.source_world_id) AS world_count,
                COUNT(*) AS total_opps,
                SUM(o.daily_profit) AS total_daily_profit,
                AVG(CASE WHEN o.home_median_price > 0
                         THEN CAST(o.unit_profit AS REAL) / o.home_median_price
                         ELSE NULL END) AS avg_gap_pct
         FROM opportunities o
         LEFT JOIN worlds w ON w.world_id = o.source_world_id
         GROUP BY dc
         ORDER BY total_daily_profit DESC"
    )
    .fetch_all(db)
    .await?;

    let dc_rankings: Vec<DcStat> = dc_rows
        .into_iter()
        .map(|r| DcStat {
            dc: r.dc.unwrap_or_default(),
            world_count: r.world_count,
            total_opps: r.total_opps,
            total_daily_profit: r.total_daily_profit.unwrap_or(0),
            avg_gap_pct: r.avg_gap_pct.unwrap_or(0.0) * 100.0,
        })
        .collect();

    // --- confidence distribution ---
    let conf_rows = sqlx::query!(
        "SELECT confidence AS label,
                COUNT(*) AS count,
                SUM(daily_profit) AS total_daily_profit
         FROM opportunities
         GROUP BY confidence
         ORDER BY count DESC"
    )
    .fetch_all(db)
    .await?;

    let confidence_dist: Vec<ConfidenceBucket> = conf_rows
        .into_iter()
        .map(|r| ConfidenceBucket {
            label: r.label,
            count: r.count,
            total_daily_profit: r.total_daily_profit.unwrap_or(0),
        })
        .collect();

    // --- top items ---
    let item_rows = sqlx::query!(
        "SELECT o.item_id,
                COALESCE(xi.name_en, '') AS name,
                COUNT(DISTINCT o.scan_id) AS scan_count,
                AVG(CAST(o.unit_profit AS REAL)) AS avg_unit_profit,
                MAX(o.daily_profit) AS max_daily_profit,
                MAX(CASE o.confidence
                    WHEN 'HOT' THEN 4 WHEN 'OK' THEN 3
                    WHEN 'COMPETITIVE' THEN 2 WHEN 'SLOW' THEN 1
                    ELSE 0 END) AS conf_rank
         FROM opportunities o
         LEFT JOIN items xi ON xi.item_id = o.item_id
         GROUP BY o.item_id
         ORDER BY scan_count DESC, avg_unit_profit DESC
         LIMIT 20"
    )
    .fetch_all(db)
    .await?;

    let top_items: Vec<TopItem> = item_rows
        .into_iter()
        .map(|r| TopItem {
            item_id: r.item_id,
            name: r.name.unwrap_or_default(),
            scan_count: r.scan_count,
            avg_unit_profit: r.avg_unit_profit.unwrap_or(0.0),
            max_daily_profit: r.max_daily_profit.unwrap_or(0),
            best_confidence: conf_label(r.conf_rank.unwrap_or(0)).to_string(),
        })
        .collect();

    // --- scan history ---
    let history_rows = sqlx::query!(
        "SELECT r.started_at,
                COALESCE(r.opportunities_found, 0) AS opportunities_found,
                COALESCE(SUM(o.daily_profit), 0) AS total_daily_profit,
                COALESCE(r.items_scanned, 0) AS items_scanned
         FROM scan_runs r
         LEFT JOIN opportunities o ON o.scan_id = r.scan_id
         WHERE r.status = 'done'
         GROUP BY r.scan_id
         ORDER BY r.started_at ASC
         LIMIT 50"
    )
    .fetch_all(db)
    .await?;

    let scan_history: Vec<ScanPoint> = history_rows
        .into_iter()
        .map(|r| ScanPoint {
            started_at: r.started_at * 1000, // → ms
            opportunities_found: r.opportunities_found.unwrap_or(0),
            total_daily_profit: r.total_daily_profit.unwrap_or(0),
            items_scanned: r.items_scanned.unwrap_or(0),
        })
        .collect();

    // --- profit ratio distribution ---
    // Buckets: <10%, 10-25%, 25-50%, 50-100%, 100-200%, >200%
    let ratio_rows = sqlx::query!(
        "SELECT
           SUM(CASE WHEN home_median_price > 0
               AND CAST(unit_profit AS REAL)/home_median_price < 0.10 THEN 1 ELSE 0 END) AS b0,
           SUM(CASE WHEN home_median_price > 0
               AND CAST(unit_profit AS REAL)/home_median_price BETWEEN 0.10 AND 0.249 THEN 1 ELSE 0 END) AS b1,
           SUM(CASE WHEN home_median_price > 0
               AND CAST(unit_profit AS REAL)/home_median_price BETWEEN 0.25 AND 0.499 THEN 1 ELSE 0 END) AS b2,
           SUM(CASE WHEN home_median_price > 0
               AND CAST(unit_profit AS REAL)/home_median_price BETWEEN 0.50 AND 0.999 THEN 1 ELSE 0 END) AS b3,
           SUM(CASE WHEN home_median_price > 0
               AND CAST(unit_profit AS REAL)/home_median_price BETWEEN 1.00 AND 1.999 THEN 1 ELSE 0 END) AS b4,
           SUM(CASE WHEN home_median_price > 0
               AND CAST(unit_profit AS REAL)/home_median_price >= 2.00 THEN 1 ELSE 0 END) AS b5
         FROM opportunities"
    )
    .fetch_one(db)
    .await?;

    let profit_ratio_dist = vec![
        RatioBucket { lower: 0.0, label: "< 10%".into(), count: ratio_rows.b0.unwrap_or(0) },
        RatioBucket { lower: 0.10, label: "10–25%".into(), count: ratio_rows.b1.unwrap_or(0) },
        RatioBucket { lower: 0.25, label: "25–50%".into(), count: ratio_rows.b2.unwrap_or(0) },
        RatioBucket { lower: 0.50, label: "50–100%".into(), count: ratio_rows.b3.unwrap_or(0) },
        RatioBucket { lower: 1.00, label: "100–200%".into(), count: ratio_rows.b4.unwrap_or(0) },
        RatioBucket { lower: 2.00, label: "> 200%".into(), count: ratio_rows.b5.unwrap_or(0) },
    ];

    Ok(MarketStats {
        total_scans,
        total_opportunities,
        best_scan_daily_profit,
        avg_opps_per_scan,
        world_rankings,
        dc_rankings,
        confidence_dist,
        top_items,
        scan_history,
        profit_ratio_dist,
    })
}
