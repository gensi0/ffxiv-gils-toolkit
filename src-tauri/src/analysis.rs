//! Glue between Universalis responses and the pure F5 math, shared by the
//! Item Detail view and the scan drill-down.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::net::now_ms;
use crate::profit::{self, Evaluation, HomeStats, ListingRec, Params, Quality, SaleRec};
use crate::universalis::{HistoryItem, MarketItem};

pub fn sales_of(history: &HistoryItem) -> Vec<SaleRec> {
    history
        .entries
        .iter()
        .map(|s| SaleRec {
            price_per_unit: s.price_per_unit,
            quantity: s.quantity,
            hq: s.hq,
            timestamp: s.timestamp,
        })
        .collect()
}

/// Copie sans mes propres annonces : elles ne sont pas de la concurrence.
pub fn without_mine(market: &MarketItem, mine: &[String]) -> MarketItem {
    let mut m = market.clone();
    if !mine.is_empty() {
        m.listings
            .retain(|l| !crate::ventes::is_mine(l.retainer_name.as_deref(), mine));
    }
    m
}

/// Listings grouped by world. `default_world` is used for world-scoped
/// responses, whose listings carry no world id.
pub fn listings_by_world(
    market: &MarketItem,
    default_world: Option<i64>,
) -> BTreeMap<i64, Vec<ListingRec>> {
    let mut out: BTreeMap<i64, Vec<ListingRec>> = BTreeMap::new();
    for l in &market.listings {
        let Some(world) = l.world_id.or(default_world).or(market.world_id) else {
            continue;
        };
        out.entry(world).or_default().push(ListingRec {
            price_per_unit: l.price_per_unit,
            quantity: l.quantity,
            hq: l.hq,
        });
    }
    out
}

/// World id → last upload (unix ms) from a DC/region response.
pub fn upload_times(market: &MarketItem) -> HashMap<i64, i64> {
    let mut out: HashMap<i64, i64> = market
        .world_upload_times
        .iter()
        .filter_map(|(k, v)| Some((k.parse().ok()?, *v)))
        .collect();
    if let Some(w) = market.world_id {
        out.entry(w).or_insert(market.last_upload_time);
    }
    out
}

pub struct ItemContext<'a> {
    pub home_world_id: i64,
    pub home_sales: &'a [SaleRec],
    pub home_listings: &'a [ListingRec],
    pub seller_tax: f64,
    pub sell_horizon_days: f64,
    pub max_data_age_ms: i64,
    pub now_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceOpportunity {
    pub world_id: i64,
    /// Quality of the units bought at the source.
    pub quality: Quality,
    #[serde(flatten)]
    pub eval: Evaluation,
    pub source_upload_ms: Option<i64>,
    pub is_stale: bool,
}

pub struct ItemAnalysis {
    pub home_nq: Option<HomeStats>,
    pub home_hq: Option<HomeStats>,
    pub sources: Vec<SourceOpportunity>,
}

/// Runs F5 for every source world (home excluded, stale-beyond-limit
/// excluded) and both source qualities.
pub fn analyze_item(
    ctx: &ItemContext<'_>,
    source_listings: &BTreeMap<i64, Vec<ListingRec>>,
    uploads: &HashMap<i64, i64>,
    p: &Params,
) -> ItemAnalysis {
    let now_s = ctx.now_ms / 1000;
    let home_nq = profit::home_stats(
        ctx.home_sales,
        ctx.home_listings,
        Quality::NQ,
        ctx.seller_tax,
        now_s,
        p,
    );
    let home_hq = profit::home_stats(
        ctx.home_sales,
        ctx.home_listings,
        Quality::HQ,
        ctx.seller_tax,
        now_s,
        p,
    );
    let mut sources = Vec::new();
    for (&world, ladder) in source_listings {
        if world == ctx.home_world_id {
            continue;
        }
        let upload = uploads.get(&world).copied();
        let age = upload.map(|u| ctx.now_ms - u);
        if age.is_some_and(|a| a > ctx.max_data_age_ms) {
            continue;
        }
        for q in [Quality::NQ, Quality::HQ] {
            if let Some(eval) = profit::evaluate_source(
                home_nq.as_ref(),
                home_hq.as_ref(),
                q,
                ladder,
                ctx.sell_horizon_days,
                p,
            ) {
                sources.push(SourceOpportunity {
                    world_id: world,
                    quality: q,
                    eval,
                    source_upload_ms: upload,
                    is_stale: age.is_none_or(|a| a > p.stale_after_secs * 1000),
                });
            }
        }
    }
    sources.sort_by(|a, b| b.eval.daily_profit.total_cmp(&a.eval.daily_profit));
    ItemAnalysis {
        home_nq,
        home_hq,
        sources,
    }
}

/// Persists sales (deduplicated) into `sales_history`.
pub async fn store_sales(pool: &SqlitePool, world_id: i64, history: &HistoryItem) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    for s in &history.entries {
        let buyer_hash = s.buyer_name.as_deref().map(hash_name);
        sqlx::query(
            "INSERT OR IGNORE INTO sales_history
               (item_id, world_id, sold_at_unix, quality, price_per_unit, quantity, buyer_hash)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(history.item_id)
        .bind(world_id)
        .bind(s.timestamp)
        .bind(Quality::of(s.hq).as_str())
        .bind(s.price_per_unit as i64)
        .bind(s.quantity)
        .bind(buyer_hash)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// FNV-1a: enough to deduplicate sales without keeping player names.
fn hash_name(name: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Per world × quality summary of a listings response, written to `price_snapshot`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualitySummary {
    pub min_price: Option<u64>,
    /// Units available at exactly the min price.
    pub units_at_min: u64,
    pub listings: u32,
    pub units_listed: u64,
}

pub fn summarize(listings: &[ListingRec], q: Quality) -> QualitySummary {
    let mine: Vec<&ListingRec> = listings.iter().filter(|l| l.hq == q.is_hq()).collect();
    let min_price = mine.iter().map(|l| l.price_per_unit).min();
    QualitySummary {
        min_price,
        units_at_min: mine
            .iter()
            .filter(|l| Some(l.price_per_unit) == min_price)
            .map(|l| u64::from(l.quantity))
            .sum(),
        listings: mine.len() as u32,
        units_listed: mine.iter().map(|l| u64::from(l.quantity)).sum(),
    }
}

pub async fn store_snapshot(
    pool: &SqlitePool,
    item_id: u32,
    world_id: i64,
    q: Quality,
    s: &QualitySummary,
    upload_ms: Option<i64>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO price_snapshot
           (item_id, world_id, quality, min_price, listings_count, units_listed,
            sale_velocity, last_upload_unix, fetched_at)
         VALUES (?, ?, ?, ?, ?, ?, NULL, ?, ?)",
    )
    .bind(item_id)
    .bind(world_id)
    .bind(q.as_str())
    .bind(s.min_price.map(|p| p as i64))
    .bind(s.listings)
    .bind(s.units_listed as i64)
    .bind(upload_ms.map(|ms| ms / 1000))
    .bind(now_ms())
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profit::PARAMS;

    const NOW_MS: i64 = 1_800_000_000_000;

    fn l(price: u64, qty: u32, hq: bool) -> ListingRec {
        ListingRec {
            price_per_unit: price,
            quantity: qty,
            hq,
        }
    }

    #[test]
    fn analyze_skips_home_and_too_old_worlds_and_flags_stale() {
        let sales: Vec<SaleRec> = (0..14)
            .map(|i| SaleRec {
                price_per_unit: 1000,
                quantity: 1,
                hq: false,
                timestamp: NOW_MS / 1000 - i * 40_000,
            })
            .collect();
        let ctx = ItemContext {
            home_world_id: 80,
            home_sales: &sales,
            home_listings: &[],
            seller_tax: 0.05,
            sell_horizon_days: 3.0,
            max_data_age_ms: 24 * 3600 * 1000,
            now_ms: NOW_MS,
        };
        let sources = BTreeMap::from([
            (80, vec![l(10, 1, false)]),  // home: never a source
            (33, vec![l(400, 2, false)]), // fresh
            (36, vec![l(300, 2, false)]), // 10h old: kept, stale badge
            (42, vec![l(200, 2, false)]), // 2 days old: dropped
        ]);
        let hour = 3600 * 1000;
        let uploads = HashMap::from([
            (33, NOW_MS - hour),
            (36, NOW_MS - 10 * hour),
            (42, NOW_MS - 48 * hour),
        ]);
        let a = analyze_item(&ctx, &sources, &uploads, &PARAMS);
        let worlds: Vec<(i64, bool)> = a.sources.iter().map(|s| (s.world_id, s.is_stale)).collect();
        assert_eq!(
            worlds,
            vec![(36, true), (33, false)],
            "sorted by daily profit"
        );
    }

    #[test]
    fn summary_counts_units_at_min() {
        let s = summarize(
            &[
                l(5, 3, false),
                l(5, 2, false),
                l(7, 9, false),
                l(1, 1, true),
            ],
            Quality::NQ,
        );
        assert_eq!(
            (s.min_price, s.units_at_min, s.listings, s.units_listed),
            (Some(5), 5, 3, 14)
        );
    }
}
