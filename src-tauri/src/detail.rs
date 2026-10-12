//! F3 Item Detail: per-world prices across the allowed scope, home-world
//! stats and the F5 arbitrage summary for one item.

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use futures::future::join_all;
use serde::Serialize;

use crate::analysis::{self, ItemContext, QualitySummary, SourceOpportunity};
use crate::error::{AppError, AppResult};
use crate::net::{now_ms, CachePolicy};
use crate::profit::{HomeStats, ListingRec, Quality, SaleRec, PARAMS};
use crate::results::item_infos;
use crate::scan;
use crate::scope;
use crate::settings;
use crate::universalis::{TTL_HISTORY, TTL_MARKET};
use crate::worlds;
use crate::xivapi::ItemInfo;
use crate::AppState;

const DETAIL_HISTORY: Duration = Duration::from_secs(28 * 24 * 3600);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldRow {
    pub world_id: i64,
    pub world: String,
    pub dc: String,
    pub is_home: bool,
    pub nq: QualitySummary,
    pub hq: QualitySummary,
    pub upload_ms: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodStats {
    pub units_per_day: f64,
    pub sales_per_day: f64,
    pub average: Option<f64>,
    pub median: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityHomeStats {
    pub d7: PeriodStats,
    pub d28: PeriodStats,
    /// F5 home-side numbers (7 days), absent when nothing sold.
    pub f5: Option<HomeStats>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArbitrageRow {
    pub world: String,
    pub dc: String,
    #[serde(flatten)]
    pub source: SourceOpportunity,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDetail {
    pub item: ItemInfo,
    pub home_world: String,
    pub seller_tax: f64,
    pub worlds: Vec<WorldRow>,
    pub home_nq: QualityHomeStats,
    pub home_hq: QualityHomeStats,
    pub arbitrage: Vec<ArbitrageRow>,
    /// DCs whose listings could not be fetched.
    pub failed_dcs: Vec<String>,
    pub fetched_at: i64,
}

fn period_stats(sales: &[SaleRec], q: Quality, days: i64, now_s: i64) -> PeriodStats {
    let since = now_s - days * 86_400;
    let mut points: Vec<(u64, u32)> = sales
        .iter()
        .filter(|s| s.hq == q.is_hq() && s.timestamp >= since)
        .map(|s| (s.price_per_unit, s.quantity))
        .collect();
    let units: u64 = points.iter().map(|&(_, q)| u64::from(q)).sum();
    let gil: u64 = points.iter().map(|&(p, q)| p * u64::from(q)).sum();
    points.sort_unstable_by_key(|&(p, _)| p);
    let mut acc = 0u64;
    let median = points
        .iter()
        .find(|&&(_, q)| {
            acc += u64::from(q);
            acc * 2 >= units
        })
        .map(|&(p, _)| p as f64);
    PeriodStats {
        units_per_day: units as f64 / days as f64,
        sales_per_day: points.len() as f64 / days as f64,
        average: (units > 0).then(|| gil as f64 / units as f64),
        median: if units > 0 { median } else { None },
    }
}

pub async fn item_detail(state: &AppState, item_id: u32, refresh: bool) -> AppResult<ItemDetail> {
    let settings = settings::load(&state.db).await?;
    let scope = scope::build(&settings, &worlds::load(&state.db).await?)?;
    let api = &state.universalis;
    let (market_policy, history_policy) = if refresh {
        (CachePolicy::Refresh, CachePolicy::Refresh)
    } else {
        (
            CachePolicy::MaxAge(TTL_MARKET),
            CachePolicy::MaxAge(TTL_HISTORY),
        )
    };

    let infos = item_infos(&state.xivapi, &[item_id], settings.language).await;
    let item = infos
        .get(&item_id)
        .cloned()
        .ok_or_else(|| AppError::Other(format!("item {item_id} not found on XIVAPI")))?;

    let ids = [item_id];
    let market_calls = scope.dcs.iter().map(|dc| async move {
        (
            dc.name.clone(),
            api.market(&dc.name, &ids, None, market_policy).await,
        )
    });
    let (markets, history, seller_tax) = tokio::join!(
        join_all(market_calls),
        api.history(&scope.home.name, &ids, DETAIL_HISTORY, history_policy),
        scan::seller_tax(api, &scope, &settings),
    );
    let history = history?.remove(&item_id);

    let mut ladders: BTreeMap<i64, Vec<ListingRec>> = BTreeMap::new();
    let mut uploads: HashMap<i64, i64> = HashMap::new();
    let mut failed_dcs = Vec::new();
    for (dc, res) in markets {
        match res {
            Ok(mut m) => {
                if let Some(item) = m.remove(&item_id) {
                    for (w, l) in analysis::listings_by_world(&item, None) {
                        ladders.entry(w).or_default().extend(l);
                    }
                    uploads.extend(analysis::upload_times(&item));
                }
            }
            Err(e) => {
                tracing::warn!(%dc, error = %e, "detail: DC listings failed");
                failed_dcs.push(dc);
            }
        }
    }

    let now = now_ms();
    let sales = history.as_ref().map(analysis::sales_of).unwrap_or_default();
    if let Some(h) = &history {
        analysis::store_sales(&state.db, scope.home.id, h).await?;
    }

    let mut world_rows = Vec::new();
    for w in &scope.worlds {
        let listings = ladders.get(&w.id).map(Vec::as_slice).unwrap_or_default();
        let upload = uploads.get(&w.id).copied();
        let nq = analysis::summarize(listings, Quality::NQ);
        let hq = analysis::summarize(listings, Quality::HQ);
        analysis::store_snapshot(&state.db, item_id, w.id, Quality::NQ, &nq, upload).await?;
        analysis::store_snapshot(&state.db, item_id, w.id, Quality::HQ, &hq, upload).await?;
        world_rows.push(WorldRow {
            world_id: w.id,
            world: w.name.clone(),
            dc: w.dc.clone(),
            is_home: w.id == scope.home.id,
            nq,
            hq,
            upload_ms: upload,
        });
    }

    let home_listings = ladders.get(&scope.home.id).cloned().unwrap_or_default();
    let ctx = ItemContext {
        home_world_id: scope.home.id,
        home_sales: &sales,
        home_listings: &home_listings,
        seller_tax,
        sell_horizon_days: f64::from(settings.sell_horizon_days),
        max_data_age_ms: i64::from(settings.max_data_age_hours) * 3600 * 1000,
        now_ms: now,
    };
    let analysis = analysis::analyze_item(&ctx, &ladders, &uploads, &PARAMS);
    let now_s = now / 1000;
    let quality_stats = |q: Quality, f5: Option<HomeStats>| QualityHomeStats {
        d7: period_stats(&sales, q, 7, now_s),
        d28: period_stats(&sales, q, 28, now_s),
        f5,
    };
    let arbitrage = analysis
        .sources
        .into_iter()
        .map(|source| {
            let w = scope.world(source.world_id);
            ArbitrageRow {
                world: w.map_or_else(|| source.world_id.to_string(), |w| w.name.clone()),
                dc: w.map(|w| w.dc.clone()).unwrap_or_default(),
                source,
            }
        })
        .collect();

    Ok(ItemDetail {
        item,
        home_world: scope.home.name.clone(),
        seller_tax,
        worlds: world_rows,
        home_nq: quality_stats(Quality::NQ, analysis.home_nq),
        home_hq: quality_stats(Quality::HQ, analysis.home_hq),
        arbitrage,
        failed_dcs,
        fetched_at: now,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn period_stats_weights_by_quantity() {
        let now = 1_800_000_000;
        let sales = [
            SaleRec {
                price_per_unit: 10,
                quantity: 9,
                hq: false,
                timestamp: now - 100,
            },
            SaleRec {
                price_per_unit: 100,
                quantity: 1,
                hq: false,
                timestamp: now - 100,
            },
            SaleRec {
                price_per_unit: 5,
                quantity: 1,
                hq: false,
                timestamp: now - 10 * 86_400,
            },
        ];
        let s = period_stats(&sales, Quality::NQ, 7, now);
        assert!((s.units_per_day - 10.0 / 7.0).abs() < 1e-9);
        assert_eq!(s.median, Some(10.0));
        assert_eq!(s.average, Some(19.0));
        let empty = period_stats(&sales, Quality::HQ, 7, now);
        assert_eq!((empty.average, empty.median), (None, None));
    }
}
