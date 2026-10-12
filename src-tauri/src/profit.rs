//! F5 profit math: pure functions, no I/O. Every threshold lives in [`Params`].
//!
//! Vocabulary: the *home* world H is where items are resold; a *source* world
//! S is where they are bought. Throughput is always measured in **units**
//! (sum of sale quantities), never in sale events.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Quality {
    NQ,
    HQ,
}

impl Quality {
    pub fn is_hq(self) -> bool {
        self == Quality::HQ
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Quality::NQ => "NQ",
            Quality::HQ => "HQ",
        }
    }
    pub fn of(hq: bool) -> Self {
        if hq {
            Quality::HQ
        } else {
            Quality::NQ
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Confidence {
    Hot,
    Ok,
    Competitive,
    Saturated,
    Slow,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::Hot => "HOT",
            Confidence::Ok => "OK",
            Confidence::Competitive => "COMPETITIVE",
            Confidence::Saturated => "SATURATED",
            Confidence::Slow => "SLOW",
        }
    }
}

/// All F5 thresholds in one place.
#[derive(Debug, Clone)]
pub struct Params {
    /// Fixed 5% the buyer pays on top of the listed price.
    pub buyer_tax: f64,
    /// Home sales window used for units/day and the median.
    pub history_window_secs: i64,
    /// Sales outside [low, high] × raw median are dropped before the median.
    pub median_low: f64,
    pub median_high: f64,
    /// Home listings below this × median are ignored as troll / mispriced.
    pub troll_floor_ratio: f64,
    /// Home listings at or below this × expected sell price count as depth.
    pub depth_ratio: f64,
    pub slow_units_per_day: f64,
    pub saturated_days: f64,
    pub competitive_days: f64,
    pub hot_units_per_day: f64,
    pub hot_days: f64,
    /// Source data older than this gets the "stale" badge.
    pub stale_after_secs: i64,
    pub inventory_slots: u32,
}

pub const PARAMS: Params = Params {
    buyer_tax: 0.05,
    history_window_secs: 7 * 24 * 3600,
    median_low: 0.2,
    median_high: 5.0,
    troll_floor_ratio: 0.5,
    depth_ratio: 1.05,
    slow_units_per_day: 0.5,
    saturated_days: 7.0,
    competitive_days: 3.0,
    hot_units_per_day: 5.0,
    hot_days: 1.0,
    stale_after_secs: 6 * 3600,
    inventory_slots: 140,
};

#[derive(Debug, Clone, Copy)]
pub struct SaleRec {
    pub price_per_unit: u64,
    pub quantity: u32,
    pub hq: bool,
    /// Unix seconds.
    pub timestamp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListingRec {
    pub price_per_unit: u64,
    pub quantity: u32,
    pub hq: bool,
}

/// Quantity-weighted median of `(price, quantity)` pairs.
fn weighted_median(points: &mut [(u64, u32)]) -> Option<f64> {
    let total: u64 = points.iter().map(|&(_, q)| u64::from(q)).sum();
    if total == 0 {
        return None;
    }
    points.sort_unstable_by_key(|&(p, _)| p);
    let half = total as f64 / 2.0;
    let mut acc = 0u64;
    for &(price, qty) in points.iter() {
        acc += u64::from(qty);
        if acc as f64 >= half {
            return Some(price as f64);
        }
    }
    points.last().map(|&(p, _)| p as f64)
}

/// Home-side numbers for one quality of one item.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeStats {
    pub quality: Quality,
    pub units_per_day: f64,
    pub sales_per_day: f64,
    pub median_7d: f64,
    pub floor_listing: Option<u64>,
    pub expected_sell: f64,
    pub net_sale: f64,
    pub depth_units: u64,
    pub days_of_supply: Option<f64>,
}

/// Home-side stats for quality `q`, or `None` when nothing sold in the window
/// (an empty home market cannot absorb anything).
pub fn home_stats(
    sales: &[SaleRec],
    listings: &[ListingRec],
    q: Quality,
    seller_tax: f64,
    now_s: i64,
    p: &Params,
) -> Option<HomeStats> {
    let since = now_s - p.history_window_secs;
    let window: Vec<&SaleRec> = sales
        .iter()
        .filter(|s| s.hq == q.is_hq() && s.timestamp >= since && s.quantity > 0)
        .collect();
    if window.is_empty() {
        return None;
    }
    let days = p.history_window_secs as f64 / 86_400.0;
    let units: u64 = window.iter().map(|s| u64::from(s.quantity)).sum();
    let units_per_day = units as f64 / days;
    let sales_per_day = window.len() as f64 / days;

    let mut all: Vec<(u64, u32)> = window
        .iter()
        .map(|s| (s.price_per_unit, s.quantity))
        .collect();
    let raw = weighted_median(&mut all)?;
    let mut kept: Vec<(u64, u32)> = all
        .into_iter()
        .filter(|&(price, _)| {
            let price = price as f64;
            price >= raw * p.median_low && price <= raw * p.median_high
        })
        .collect();
    let median_7d = weighted_median(&mut kept).unwrap_or(raw);

    let floor_listing = listings
        .iter()
        .filter(|l| l.hq == q.is_hq() && l.price_per_unit as f64 >= median_7d * p.troll_floor_ratio)
        .map(|l| l.price_per_unit)
        .min();
    // You must undercut the cheapest credible listing to sell.
    let expected_sell = match floor_listing {
        Some(floor) => median_7d.min(floor.saturating_sub(1) as f64),
        None => median_7d,
    }
    .max(1.0);
    let net_sale = expected_sell * (1.0 - seller_tax);

    let depth_units: u64 = listings
        .iter()
        .filter(|l| l.hq == q.is_hq() && l.price_per_unit as f64 <= expected_sell * p.depth_ratio)
        .map(|l| u64::from(l.quantity))
        .sum();
    let days_of_supply = (units_per_day > 0.0).then(|| depth_units as f64 / units_per_day);

    Some(HomeStats {
        quality: q,
        units_per_day,
        sales_per_day,
        median_7d,
        floor_listing,
        expected_sell,
        net_sale,
        depth_units,
        days_of_supply,
    })
}

pub fn confidence(units_per_day: f64, days_of_supply: Option<f64>, p: &Params) -> Confidence {
    let supply = days_of_supply.unwrap_or(f64::INFINITY);
    if units_per_day < p.slow_units_per_day {
        Confidence::Slow
    } else if supply > p.saturated_days {
        Confidence::Saturated
    } else if supply > p.competitive_days {
        Confidence::Competitive
    } else if units_per_day >= p.hot_units_per_day && supply < p.hot_days {
        Confidence::Hot
    } else {
        Confidence::Ok
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Purchase {
    pub units: u64,
    /// Including the buyer tax.
    pub cost: f64,
    pub cheapest_unit_price: u64,
    pub listings: usize,
}

/// Buys whole listings, cheapest first, while they stay under `breakeven`
/// and the running total stays within `max_units`. A listing too big to fit
/// is skipped (not bought partially); later, smaller ones may still fit.
pub fn plan_purchase(
    ladder: &[ListingRec],
    breakeven: f64,
    max_units: u64,
    p: &Params,
) -> Purchase {
    let mut sorted: Vec<&ListingRec> = ladder.iter().collect();
    sorted.sort_by_key(|l| l.price_per_unit);
    let mut out = Purchase {
        units: 0,
        cost: 0.0,
        cheapest_unit_price: 0,
        listings: 0,
    };
    let mut spent = 0u64;
    for l in sorted {
        if l.price_per_unit as f64 >= breakeven {
            break;
        }
        let qty = u64::from(l.quantity);
        if qty == 0 || out.units + qty > max_units {
            continue;
        }
        if out.listings == 0 {
            out.cheapest_unit_price = l.price_per_unit;
        }
        out.units += qty;
        spent += l.price_per_unit * qty;
        out.listings += 1;
    }
    out.cost = spent as f64 * (1.0 + p.buyer_tax);
    out
}

/// F5 outputs for one source ladder against one home demand.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Evaluation {
    /// Home demand the units are sold into (HQ can be sold as NQ).
    pub sold_as: Quality,
    pub units_to_buy: u64,
    pub source_unit_price: u64,
    pub source_cost: f64,
    pub expected_sell: f64,
    pub home_median: f64,
    pub home_units_per_day: f64,
    pub days_of_supply: Option<f64>,
    pub unit_profit: f64,
    pub profit_per_trip: f64,
    pub daily_profit: f64,
    pub confidence: Confidence,
}

pub fn evaluate(
    home: &HomeStats,
    ladder: &[ListingRec],
    sell_horizon_days: f64,
    p: &Params,
) -> Option<Evaluation> {
    if home.units_per_day <= 0.0 || sell_horizon_days <= 0.0 {
        return None;
    }
    let breakeven = home.net_sale / (1.0 + p.buyer_tax);
    let max_units = (home.units_per_day * sell_horizon_days).floor() as u64;
    let buy = plan_purchase(ladder, breakeven, max_units, p);
    if buy.units == 0 {
        return None;
    }
    let profit_per_trip = buy.units as f64 * home.net_sale - buy.cost;
    if profit_per_trip <= 0.0 {
        return None;
    }
    let unit_profit = profit_per_trip / buy.units as f64;
    let daily_units = home.units_per_day.min(buy.units as f64 / sell_horizon_days);
    Some(Evaluation {
        sold_as: home.quality,
        units_to_buy: buy.units,
        source_unit_price: buy.cheapest_unit_price,
        source_cost: buy.cost,
        expected_sell: home.expected_sell,
        home_median: home.median_7d,
        home_units_per_day: home.units_per_day,
        days_of_supply: home.days_of_supply,
        unit_profit,
        profit_per_trip,
        daily_profit: unit_profit * daily_units,
        confidence: confidence(home.units_per_day, home.days_of_supply, p),
    })
}

/// Best evaluation for a source ladder of quality `source_q`: NQ is sold as
/// NQ; HQ is sold as HQ or into NQ demand, whichever earns more per trip.
pub fn evaluate_source(
    home_nq: Option<&HomeStats>,
    home_hq: Option<&HomeStats>,
    source_q: Quality,
    ladder: &[ListingRec],
    sell_horizon_days: f64,
    p: &Params,
) -> Option<Evaluation> {
    let ladder: Vec<ListingRec> = ladder
        .iter()
        .copied()
        .filter(|l| l.hq == source_q.is_hq())
        .collect();
    let targets: &[Option<&HomeStats>] = match source_q {
        Quality::NQ => &[home_nq],
        Quality::HQ => &[home_hq, home_nq],
    };
    targets
        .iter()
        .flatten()
        .filter_map(|home| evaluate(home, &ladder, sell_horizon_days, p))
        .max_by(|a, b| a.profit_per_trip.total_cmp(&b.profit_per_trip))
}

/// Scan prefilter (F4 step 5), from aggregated data only.
/// Returns `(est_profit, est_score)` when the candidate is worth drilling into.
pub fn prefilter(
    est_sell: f64,
    home_units_per_day: f64,
    remote_min: f64,
    seller_tax: f64,
    min_margin: f64,
    min_units_per_day: f64,
    p: &Params,
) -> Option<(f64, f64)> {
    if est_sell <= 0.0 || remote_min <= 0.0 || home_units_per_day < min_units_per_day {
        return None;
    }
    let est_cost = remote_min * (1.0 + p.buyer_tax);
    let est_profit = est_sell * (1.0 - seller_tax) - est_cost;
    if est_profit <= 0.0 || est_profit / est_cost < min_margin {
        return None;
    }
    Some((est_profit, est_profit * home_units_per_day))
}

/// One opportunity as seen by the trip planner.
#[derive(Debug, Clone, Copy)]
pub struct TripItem {
    pub units: u64,
    pub stack_size: u32,
    pub profit_per_trip: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TripPlan {
    /// Indices into the input, in pick order.
    pub picked: Vec<usize>,
    pub slots_used: u32,
    pub profit_per_trip: f64,
    pub profit_per_hour: f64,
}

/// Fills the inventory (140 slots) with the most profitable items first and
/// computes profit per hour for one round trip of `travel_minutes`.
pub fn plan_trip(items: &[TripItem], travel_minutes: f64, p: &Params) -> TripPlan {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| {
        items[b]
            .profit_per_trip
            .total_cmp(&items[a].profit_per_trip)
    });
    let mut plan = TripPlan {
        picked: Vec::new(),
        slots_used: 0,
        profit_per_trip: 0.0,
        profit_per_hour: 0.0,
    };
    for i in order {
        let it = items[i];
        let slots = it.units.div_ceil(u64::from(it.stack_size.max(1))) as u32;
        if plan.slots_used + slots > p.inventory_slots {
            continue;
        }
        plan.slots_used += slots;
        plan.profit_per_trip += it.profit_per_trip;
        plan.picked.push(i);
    }
    if travel_minutes > 0.0 {
        plan.profit_per_hour = plan.profit_per_trip / (travel_minutes / 60.0);
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;
    const DAY: i64 = 86_400;

    fn sale(price: u64, qty: u32, hq: bool, days_ago: f64) -> SaleRec {
        SaleRec {
            price_per_unit: price,
            quantity: qty,
            hq,
            timestamp: NOW - (days_ago * DAY as f64) as i64,
        }
    }
    fn nq(price: u64, qty: u32) -> ListingRec {
        ListingRec {
            price_per_unit: price,
            quantity: qty,
            hq: false,
        }
    }
    fn hq(price: u64, qty: u32) -> ListingRec {
        ListingRec {
            price_per_unit: price,
            quantity: qty,
            hq: true,
        }
    }
    /// 14 NQ sales of 1 unit at 1000 over the week: 2 units/day.
    fn steady_sales() -> Vec<SaleRec> {
        (0..14)
            .map(|i| sale(1000, 1, false, i as f64 * 0.5))
            .collect()
    }

    #[test]
    fn troll_listing_on_home_is_ignored() {
        // A 1-gil listing must not drag the expected sell price down.
        let listings = [nq(1, 1), nq(950, 2), nq(1200, 5)];
        let s = home_stats(&steady_sales(), &listings, Quality::NQ, 0.05, NOW, &PARAMS).unwrap();
        assert_eq!(s.median_7d, 1000.0);
        assert_eq!(s.floor_listing, Some(950));
        assert_eq!(s.expected_sell, 949.0);
        assert!((s.net_sale - 949.0 * 0.95).abs() < 1e-9);
    }

    #[test]
    fn outlier_sales_do_not_move_the_median() {
        let mut sales = steady_sales();
        sales.push(sale(999_999, 1, false, 1.0));
        sales.push(sale(2, 1, false, 2.0));
        let s = home_stats(&sales, &[], Quality::NQ, 0.0, NOW, &PARAMS).unwrap();
        assert_eq!(s.median_7d, 1000.0);
    }

    #[test]
    fn stack_size_sales_count_units_not_events() {
        // 7 sales of 99 units in a week: 1 sale/day but 99 units/day.
        let sales: Vec<SaleRec> = (0..7).map(|i| sale(10, 99, false, i as f64)).collect();
        let s = home_stats(&sales, &[], Quality::NQ, 0.0, NOW, &PARAMS).unwrap();
        assert!((s.sales_per_day - 1.0).abs() < 1e-9);
        assert!((s.units_per_day - 99.0).abs() < 1e-9);
    }

    #[test]
    fn sales_outside_window_are_ignored() {
        let sales = [sale(1000, 1, false, 8.0)];
        assert!(home_stats(&sales, &[], Quality::NQ, 0.0, NOW, &PARAMS).is_none());
    }

    #[test]
    fn empty_home_market_yields_nothing() {
        assert!(home_stats(&[], &[nq(500, 3)], Quality::NQ, 0.05, NOW, &PARAMS).is_none());
        assert!(evaluate_source(None, None, Quality::NQ, &[nq(1, 99)], 3.0, &PARAMS).is_none());
    }

    #[test]
    fn zero_velocity_yields_nothing() {
        let home = HomeStats {
            quality: Quality::NQ,
            units_per_day: 0.0,
            sales_per_day: 0.0,
            median_7d: 1000.0,
            floor_listing: None,
            expected_sell: 1000.0,
            net_sale: 950.0,
            depth_units: 0,
            days_of_supply: None,
        };
        assert!(evaluate(&home, &[nq(100, 1)], 3.0, &PARAMS).is_none());
    }

    #[test]
    fn whole_listing_purchase_constraint() {
        // 2 units/day × 3 days = at most 6 units. The 10-stack is cheapest but
        // cannot be split, so it is skipped and smaller listings are taken.
        let ladder = [nq(100, 10), nq(200, 4), nq(250, 2), nq(300, 1)];
        let buy = plan_purchase(&ladder, 900.0, 6, &PARAMS);
        assert_eq!(buy.units, 6);
        assert_eq!(buy.listings, 2);
        assert_eq!(buy.cheapest_unit_price, 200);
        assert!((buy.cost - (200.0 * 4.0 + 250.0 * 2.0) * 1.05).abs() < 1e-6);
    }

    #[test]
    fn purchase_stops_at_breakeven() {
        let ladder = [nq(100, 1), nq(500, 1), nq(600, 1)];
        let buy = plan_purchase(&ladder, 500.0, 10, &PARAMS);
        assert_eq!(
            buy.units, 1,
            "a listing priced exactly at breakeven earns nothing"
        );
    }

    #[test]
    fn full_evaluation_matches_hand_computation() {
        let home = home_stats(
            &steady_sales(),
            &[nq(1100, 1)],
            Quality::NQ,
            0.05,
            NOW,
            &PARAMS,
        )
        .unwrap();
        // expected_sell = min(1000, 1099) = 1000; net = 950; breakeven ≈ 904.76
        let e = evaluate(&home, &[nq(500, 3), nq(600, 5)], 3.0, &PARAMS).unwrap();
        // max_units = floor(2 × 3) = 6 → 3 @500, then 5 @600 does not fit.
        assert_eq!(e.units_to_buy, 3);
        let cost = 1500.0 * 1.05;
        assert!((e.source_cost - cost).abs() < 1e-6);
        assert!((e.profit_per_trip - (3.0 * 950.0 - cost)).abs() < 1e-6);
        assert!((e.unit_profit - (950.0 - 525.0)).abs() < 1e-6);
        // daily units = min(2, 3/3) = 1
        assert!((e.daily_profit - e.unit_profit).abs() < 1e-6);
        assert_eq!(e.confidence, Confidence::Ok);
    }

    #[test]
    fn hq_can_be_sold_into_nq_demand() {
        // No HQ demand at home, strong NQ demand: an HQ source still qualifies.
        let home_nq = home_stats(&steady_sales(), &[], Quality::NQ, 0.0, NOW, &PARAMS).unwrap();
        let e = evaluate_source(
            Some(&home_nq),
            None,
            Quality::HQ,
            &[hq(400, 2), nq(1, 99)],
            3.0,
            &PARAMS,
        )
        .unwrap();
        assert_eq!(e.sold_as, Quality::NQ);
        assert_eq!(
            e.units_to_buy, 2,
            "only HQ listings are part of an HQ ladder"
        );
        assert_eq!(e.source_unit_price, 400);
    }

    #[test]
    fn hq_picks_the_more_profitable_demand() {
        let hq_sales: Vec<SaleRec> = (0..14)
            .map(|i| sale(3000, 1, true, i as f64 * 0.5))
            .collect();
        let home_hq = home_stats(&hq_sales, &[], Quality::HQ, 0.0, NOW, &PARAMS).unwrap();
        let home_nq = home_stats(&steady_sales(), &[], Quality::NQ, 0.0, NOW, &PARAMS).unwrap();
        let e = evaluate_source(
            Some(&home_nq),
            Some(&home_hq),
            Quality::HQ,
            &[hq(400, 2)],
            3.0,
            &PARAMS,
        )
        .unwrap();
        assert_eq!(e.sold_as, Quality::HQ);
    }

    #[test]
    fn nq_cannot_be_sold_as_hq() {
        let hq_sales: Vec<SaleRec> = (0..14)
            .map(|i| sale(3000, 1, true, i as f64 * 0.5))
            .collect();
        let home_hq = home_stats(&hq_sales, &[], Quality::HQ, 0.0, NOW, &PARAMS).unwrap();
        assert!(evaluate_source(
            None,
            Some(&home_hq),
            Quality::NQ,
            &[nq(10, 1)],
            3.0,
            &PARAMS
        )
        .is_none());
    }

    #[test]
    fn confidence_buckets() {
        let c = |upd, dos| confidence(upd, dos, &PARAMS);
        assert_eq!(c(0.3, Some(0.0)), Confidence::Slow);
        assert_eq!(c(2.0, Some(8.0)), Confidence::Saturated);
        assert_eq!(c(2.0, Some(4.0)), Confidence::Competitive);
        assert_eq!(c(6.0, Some(0.5)), Confidence::Hot);
        assert_eq!(c(2.0, Some(0.5)), Confidence::Ok);
    }

    #[test]
    fn prefilter_rules() {
        // sell 1000, no tax, remote 500 → cost 525, profit 475, margin 0.9
        let (profit, score) = prefilter(1000.0, 2.0, 500.0, 0.0, 0.2, 1.0, &PARAMS).unwrap();
        assert!((profit - 475.0).abs() < 1e-9);
        assert!((score - 950.0).abs() < 1e-9);
        assert!(
            prefilter(1000.0, 0.5, 500.0, 0.0, 0.2, 1.0, &PARAMS).is_none(),
            "too slow"
        );
        assert!(
            prefilter(1000.0, 2.0, 850.0, 0.05, 0.2, 1.0, &PARAMS).is_none(),
            "margin too thin"
        );
    }

    #[test]
    fn trip_respects_inventory_slots() {
        let items = [
            TripItem {
                units: 999 * 100,
                stack_size: 999,
                profit_per_trip: 1_000_000.0,
            }, // 100 slots
            TripItem {
                units: 999 * 50,
                stack_size: 999,
                profit_per_trip: 500_000.0,
            }, // 50: no room
            TripItem {
                units: 30,
                stack_size: 1,
                profit_per_trip: 300_000.0,
            }, // 30 slots
        ];
        let plan = plan_trip(&items, 10.0, &PARAMS);
        assert_eq!(plan.picked, vec![0, 2]);
        assert_eq!(plan.slots_used, 130);
        assert!((plan.profit_per_hour - 1_300_000.0 * 6.0).abs() < 1e-6);
    }
}
