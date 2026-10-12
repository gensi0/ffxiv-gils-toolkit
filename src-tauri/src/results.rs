//! Read side of scans: runs, opportunities, prefilter candidates and
//! per-source-world trip plans.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::profit::{self, TripItem, PARAMS};
use crate::settings::{Language, Settings};
use crate::worlds::World;
use crate::xivapi::{ItemInfo, XivApi};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ScanRun {
    pub scan_id: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub items_scanned: Option<i64>,
    pub items_failed: Option<i64>,
    pub opportunities_found: Option<i64>,
    pub status: String,
    pub error: Option<String>,
}

const RUN_COLUMNS: &str = "scan_id, started_at, completed_at, items_scanned, items_failed,
    opportunities_found, status, error";

pub async fn list_runs(db: &SqlitePool, limit: i64) -> AppResult<Vec<ScanRun>> {
    Ok(sqlx::query_as(&format!(
        "SELECT {RUN_COLUMNS} FROM scan_runs ORDER BY started_at DESC LIMIT ?"
    ))
    .bind(limit)
    .fetch_all(db)
    .await?)
}

async fn run_by_id(db: &SqlitePool, scan_id: Option<&str>) -> AppResult<Option<ScanRun>> {
    let run = match scan_id {
        Some(id) => {
            sqlx::query_as(&format!(
                "SELECT {RUN_COLUMNS} FROM scan_runs WHERE scan_id = ?"
            ))
            .bind(id)
            .fetch_optional(db)
            .await?
        }
        None => {
            sqlx::query_as(&format!(
                "SELECT {RUN_COLUMNS} FROM scan_runs WHERE status = 'done'
                 ORDER BY started_at DESC LIMIT 1"
            ))
            .fetch_optional(db)
            .await?
        }
    };
    Ok(run)
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct OppRow {
    item_id: i64,
    quality: String,
    sold_as: String,
    source_world_id: i64,
    source_unit_price: i64,
    units_to_buy: i64,
    source_cost: i64,
    expected_sell_price: i64,
    home_median_price: i64,
    home_units_per_day: f64,
    days_of_supply: Option<f64>,
    unit_profit: i64,
    profit_per_trip: i64,
    daily_profit: i64,
    confidence: String,
    source_upload_unix: Option<i64>,
    is_stale: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityView {
    pub item: ItemInfo,
    pub quality: String,
    pub sold_as: String,
    pub source_world_id: i64,
    pub source_world: String,
    pub source_dc: String,
    pub cross_region: bool,
    pub source_unit_price: i64,
    pub units_to_buy: i64,
    pub source_cost: i64,
    pub expected_sell_price: i64,
    pub home_median_price: i64,
    pub home_units_per_day: f64,
    pub days_of_supply: Option<f64>,
    pub unit_profit: i64,
    pub profit_per_trip: i64,
    pub daily_profit: i64,
    pub confidence: String,
    pub source_upload_ms: Option<i64>,
    pub is_stale: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TripView {
    pub world_id: i64,
    pub world: String,
    pub dc: String,
    pub travel_minutes: u32,
    /// Indices into `opportunities` of the items that fit the inventory.
    pub picked: Vec<usize>,
    pub slots_used: u32,
    pub profit_per_trip: i64,
    pub profit_per_hour: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResults {
    pub run: ScanRun,
    pub home_world: String,
    pub opportunities: Vec<OpportunityView>,
    pub trips: Vec<TripView>,
}

fn placeholder_item(id: i64) -> ItemInfo {
    ItemInfo {
        id: id as u32,
        name: format!("Item #{id}"),
        icon_path: None,
        category: None,
        stack_size: 1,
        level_item: None,
        can_be_hq: false,
    }
}

pub async fn item_infos(xivapi: &XivApi, ids: &[u32], lang: Language) -> HashMap<u32, ItemInfo> {
    match xivapi.items(ids).await {
        Ok(rows) => rows
            .into_iter()
            .map(|(id, r)| (id, r.localize(lang)))
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, "item metadata unavailable");
            HashMap::new()
        }
    }
}

pub async fn scan_results(
    db: &SqlitePool,
    xivapi: &XivApi,
    settings: &Settings,
    worlds: &[World],
    scan_id: Option<&str>,
) -> AppResult<Option<ScanResults>> {
    let Some(run) = run_by_id(db, scan_id).await? else {
        return Ok(None);
    };
    let rows: Vec<OppRow> = sqlx::query_as(
        "SELECT item_id, quality, sold_as, source_world_id, source_unit_price, units_to_buy,
                source_cost, expected_sell_price, home_median_price, home_units_per_day,
                days_of_supply, unit_profit, profit_per_trip, daily_profit, confidence,
                source_upload_unix, is_stale
         FROM opportunities WHERE scan_id = ? ORDER BY daily_profit DESC",
    )
    .bind(&run.scan_id)
    .fetch_all(db)
    .await?;

    let ids: Vec<u32> = {
        let mut v: Vec<u32> = rows.iter().map(|r| r.item_id as u32).collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    let infos = item_infos(xivapi, &ids, settings.language).await;
    let home = worlds.iter().find(|w| Some(w.id) == settings.home_world_id);
    let world_of = |id: i64| worlds.iter().find(|w| w.id == id);

    let opportunities: Vec<OpportunityView> = rows
        .into_iter()
        .map(|r| {
            let w = world_of(r.source_world_id);
            OpportunityView {
                item: infos
                    .get(&(r.item_id as u32))
                    .cloned()
                    .unwrap_or_else(|| placeholder_item(r.item_id)),
                quality: r.quality,
                sold_as: r.sold_as,
                source_world_id: r.source_world_id,
                source_world: w.map_or_else(|| r.source_world_id.to_string(), |w| w.name.clone()),
                source_dc: w.map(|w| w.dc.clone()).unwrap_or_default(),
                cross_region: matches!((w, home), (Some(w), Some(h)) if w.region != h.region),
                source_unit_price: r.source_unit_price,
                units_to_buy: r.units_to_buy,
                source_cost: r.source_cost,
                expected_sell_price: r.expected_sell_price,
                home_median_price: r.home_median_price,
                home_units_per_day: r.home_units_per_day,
                days_of_supply: r.days_of_supply,
                unit_profit: r.unit_profit,
                profit_per_trip: r.profit_per_trip,
                daily_profit: r.daily_profit,
                confidence: r.confidence,
                source_upload_ms: r.source_upload_unix.map(|s| s * 1000),
                is_stale: r.is_stale,
            }
        })
        .collect();

    let trips = plan_trips(&opportunities, settings);
    Ok(Some(ScanResults {
        run,
        home_world: home.map(|h| h.name.clone()).unwrap_or_default(),
        opportunities,
        trips,
    }))
}

/// One trip per source world, sorted by profit per hour.
pub fn plan_trips(opps: &[OpportunityView], settings: &Settings) -> Vec<TripView> {
    let mut by_world: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for (i, o) in opps.iter().enumerate() {
        by_world.entry(o.source_world_id).or_default().push(i);
    }
    let mut trips: Vec<TripView> = by_world
        .into_iter()
        .map(|(world_id, idx)| {
            let first = &opps[idx[0]];
            let travel_minutes = if first.cross_region {
                settings.travel_minutes_cross_region
            } else {
                settings.travel_minutes_in_region
            };
            let items: Vec<TripItem> = idx
                .iter()
                .map(|&i| TripItem {
                    units: opps[i].units_to_buy.max(0) as u64,
                    stack_size: opps[i].item.stack_size,
                    profit_per_trip: opps[i].profit_per_trip as f64,
                })
                .collect();
            let plan = profit::plan_trip(&items, f64::from(travel_minutes), &PARAMS);
            TripView {
                world_id,
                world: first.source_world.clone(),
                dc: first.source_dc.clone(),
                travel_minutes,
                picked: plan.picked.iter().map(|&k| idx[k]).collect(),
                slots_used: plan.slots_used,
                profit_per_trip: plan.profit_per_trip.round() as i64,
                profit_per_hour: plan.profit_per_hour.round() as i64,
            }
        })
        .collect();
    trips.sort_by_key(|t| std::cmp::Reverse(t.profit_per_hour));
    trips
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct CandidateRow {
    item_id: i64,
    quality: String,
    sold_as: String,
    source_dc: String,
    source_world_id: i64,
    source_min_price: i64,
    est_sell_price: i64,
    est_profit: i64,
    est_score: f64,
    home_units_per_day: f64,
    source_upload_unix: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateView {
    pub item: ItemInfo,
    pub quality: String,
    pub sold_as: String,
    pub source_dc: String,
    pub source_world: String,
    pub source_min_price: i64,
    pub est_sell_price: i64,
    pub est_profit: i64,
    pub est_score: f64,
    pub home_units_per_day: f64,
    pub source_upload_ms: Option<i64>,
}

/// Top prefilter candidates of a scan (names only for items already cached).
pub async fn scan_candidates(
    db: &SqlitePool,
    xivapi: &XivApi,
    settings: &Settings,
    worlds: &[World],
    scan_id: &str,
    limit: i64,
) -> AppResult<Vec<CandidateView>> {
    let rows: Vec<CandidateRow> = sqlx::query_as(
        "SELECT item_id, quality, sold_as, source_dc, source_world_id, source_min_price,
                est_sell_price, est_profit, est_score, home_units_per_day, source_upload_unix
         FROM scan_candidates WHERE scan_id = ? ORDER BY est_score DESC LIMIT ?",
    )
    .bind(scan_id)
    .bind(limit)
    .fetch_all(db)
    .await?;
    let mut ids: Vec<u32> = rows.iter().map(|r| r.item_id as u32).collect();
    ids.sort_unstable();
    ids.dedup();
    let infos = item_infos(xivapi, &ids, settings.language).await;
    Ok(rows
        .into_iter()
        .map(|r| CandidateView {
            item: infos
                .get(&(r.item_id as u32))
                .cloned()
                .unwrap_or_else(|| placeholder_item(r.item_id)),
            quality: r.quality,
            sold_as: r.sold_as,
            source_dc: r.source_dc,
            source_world: worlds
                .iter()
                .find(|w| w.id == r.source_world_id)
                .map_or_else(|| r.source_world_id.to_string(), |w| w.name.clone()),
            source_min_price: r.source_min_price,
            est_sell_price: r.est_sell_price,
            est_profit: r.est_profit,
            est_score: r.est_score,
            home_units_per_day: r.home_units_per_day,
            source_upload_ms: r.source_upload_unix.map(|s| s * 1000),
        })
        .collect())
}
