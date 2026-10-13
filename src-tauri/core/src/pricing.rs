//! Rentabilité : combien de gils rapporte une unité de monnaie dépensée.

use std::collections::HashMap;

use serde::Serialize;

use crate::dataset::{Dataset, Unlock, VentureClass};
use crate::universalis::{Market, QualityMarket};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricedOffer {
    pub item: u32,
    pub name: String,
    pub icon: Option<String>,
    pub quantity: u32,
    pub cost: u32,
    pub hq: bool,
    pub shops: Vec<String>,
    pub vendors: Vec<String>,
    pub unlock: Option<Unlock>,
    pub min_listing: Option<f64>,
    pub average_sale: Option<f64>,
    pub last_sale_ms: Option<i64>,
    pub daily_velocity: Option<f64>,
    /// Prix unitaire retenu pour l'estimation.
    pub unit_price: Option<f64>,
    /// Gils estimés par unité de monnaie dépensée.
    pub gil_per_currency: Option<f64>,
}

/// Prix unitaire prudent : le plus bas entre l'annonce la moins chère et le
/// prix moyen des ventes. Une annonce isolée très haute ne gonfle pas
/// l'estimation, une moyenne ancienne non plus.
pub fn unit_price(q: &QualityMarket) -> Option<f64> {
    match (q.min_listing, q.average_sale) {
        (Some(l), Some(a)) => Some(l.min(a)),
        (Some(p), None) | (None, Some(p)) => Some(p),
        (None, None) => q.last_sale,
    }
}

pub fn price_offers(
    ds: &Dataset,
    currency: u32,
    markets: &HashMap<u32, Market>,
) -> Vec<PricedOffer> {
    let empty = Market::default();
    let mut out: Vec<PricedOffer> = ds
        .offers
        .iter()
        .filter(|o| o.currency == currency)
        .map(|o| {
            let market = markets.get(&o.item).unwrap_or(&empty);
            let q = if o.hq { &market.hq } else { &market.nq };
            let price = unit_price(q);
            let meta = ds.items.get(&o.item);
            PricedOffer {
                item: o.item,
                name: meta.map_or_else(|| format!("Objet n°{}", o.item), |m| m.name.clone()),
                icon: meta.and_then(|m| m.icon.clone()),
                quantity: o.quantity,
                cost: o.cost,
                hq: o.hq,
                shops: o.shops.clone(),
                vendors: o.vendors.clone(),
                unlock: o.unlock.clone(),
                min_listing: q.min_listing,
                average_sale: q.average_sale,
                last_sale_ms: q.last_sale_ms,
                daily_velocity: q.daily_velocity,
                unit_price: price,
                gil_per_currency: price
                    .map(|p| p * f64::from(o.quantity) / f64::from(o.cost.max(1))),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.gil_per_currency
            .unwrap_or(-1.0)
            .total_cmp(&a.gil_per_currency.unwrap_or(-1.0))
    });
    out
}

/// Taxe de l'hôtel des ventes (5 % dans la plupart des cités).
pub const MARKET_TAX: f64 = 0.05;

/// Ventes par jour minimales pour qu'un objet compte comme écoulable.
pub const MIN_VELOCITY: f64 = 1.0;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricedGilOffer {
    pub item: u32,
    pub name: String,
    pub icon: Option<String>,
    pub npc_price: u32,
    pub vendors: Vec<String>,
    pub unlock: Option<Unlock>,
    pub unit_price: Option<f64>,
    pub min_listing: Option<f64>,
    pub average_sale: Option<f64>,
    pub daily_velocity: Option<f64>,
    /// Bénéfice par unité, taxe déduite.
    pub profit: Option<f64>,
    /// Bénéfice par unité ÷ prix PNJ.
    pub margin: Option<f64>,
    /// Bénéfice × ventes par jour : ce que l'objet peut rapporter chaque jour.
    pub daily_profit: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricedDelivery {
    pub item: u32,
    pub name: String,
    pub icon: Option<String>,
    pub item_level: u32,
    pub seals: u32,
    /// Annonce la moins chère (NQ ou HQ, les sceaux sont les mêmes).
    pub buy_price: Option<f64>,
    pub daily_velocity: Option<f64>,
    /// Gils que valent les sceaux reçus, au meilleur échange du moment.
    pub seals_value: Option<f64>,
    /// Acheter à l'HV puis livrer : gain.
    pub profit: Option<f64>,
    /// Gils payés par sceau obtenu.
    pub gil_per_seal: Option<f64>,
}

fn meta(ds: &Dataset, item: u32) -> (String, Option<String>) {
    ds.items.get(&item).map_or_else(
        || (format!("Objet n°{item}"), None),
        |m| (m.name.clone(), m.icon.clone()),
    )
}

fn velocity(m: &Market) -> Option<f64> {
    match (m.nq.daily_velocity, m.hq.daily_velocity) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0.0) + b.unwrap_or(0.0)),
    }
}

pub fn price_gil_offers(ds: &Dataset, markets: &HashMap<u32, Market>) -> Vec<PricedGilOffer> {
    let empty = Market::default();
    let mut out: Vec<PricedGilOffer> = ds
        .gil_offers
        .iter()
        .map(|o| {
            let m = markets.get(&o.item).unwrap_or(&empty);
            // Un objet de PNJ est toujours NQ.
            let unit = unit_price(&m.nq);
            let profit = unit.map(|p| p * (1.0 - MARKET_TAX) - f64::from(o.price));
            let (name, icon) = meta(ds, o.item);
            PricedGilOffer {
                item: o.item,
                name,
                icon,
                npc_price: o.price,
                vendors: o.vendors.clone(),
                unlock: o.unlock.clone(),
                unit_price: unit,
                min_listing: m.nq.min_listing,
                average_sale: m.nq.average_sale,
                daily_velocity: m.nq.daily_velocity,
                profit,
                margin: profit.map(|p| p / f64::from(o.price.max(1))),
                daily_profit: profit.zip(m.nq.daily_velocity).map(|(p, v)| p * v),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.daily_profit
            .unwrap_or(f64::MIN)
            .total_cmp(&a.daily_profit.unwrap_or(f64::MIN))
    });
    out
}

/// Monnaie qu'un échange doit pouvoir absorber chaque jour (ventes par jour ×
/// coût) pour servir de référence : une livraison rapporte des milliers de
/// sceaux, un objet qui ne s'en écoule que quelques centaines n'est pas réaliste.
pub const MIN_DAILY_CURRENCY: f64 = 10_000.0;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrencyValue {
    /// Gils par unité de monnaie, taxe déduite.
    pub gil: f64,
    /// Objet de référence et son coût.
    pub item_name: String,
    pub cost: u32,
}

/// Valeur d'une unité de monnaie : le meilleur échange capable d'absorber
/// [`MIN_DAILY_CURRENCY`] par jour. Les échanges à débloquer comptent (on
/// suppose le grade ou la quête obtenus).
pub fn currency_value(
    ds: &Dataset,
    currency: u32,
    markets: &HashMap<u32, Market>,
) -> Option<CurrencyValue> {
    price_offers(ds, currency, markets)
        .into_iter()
        .filter(|o| {
            o.daily_velocity.unwrap_or(0.0) * f64::from(o.cost) / f64::from(o.quantity.max(1))
                >= MIN_DAILY_CURRENCY
        })
        .filter_map(|o| {
            o.gil_per_currency.map(|g| CurrencyValue {
                gil: g * (1.0 - MARKET_TAX),
                item_name: o.name,
                cost: o.cost,
            })
        })
        .max_by(|a, b| a.gil.total_cmp(&b.gil))
}

pub fn price_deliveries(
    ds: &Dataset,
    markets: &HashMap<u32, Market>,
    seal_value: Option<f64>,
) -> Vec<PricedDelivery> {
    let empty = Market::default();
    let mut out: Vec<PricedDelivery> = ds
        .deliveries
        .iter()
        .map(|d| {
            let m = markets.get(&d.item).unwrap_or(&empty);
            let buy = match (m.nq.min_listing, m.hq.min_listing) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            let value = seal_value.map(|v| v * f64::from(d.seals));
            let (name, icon) = meta(ds, d.item);
            PricedDelivery {
                item: d.item,
                name,
                icon,
                item_level: d.item_level,
                seals: d.seals,
                buy_price: buy,
                daily_velocity: velocity(m),
                seals_value: value,
                profit: buy.zip(value).map(|(b, v)| v - b),
                gil_per_seal: buy.map(|b| b / f64::from(d.seals)),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        a.gil_per_seal
            .unwrap_or(f64::MAX)
            .total_cmp(&b.gil_per_seal.unwrap_or(f64::MAX))
    });
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricedVenture {
    pub item: u32,
    pub name: String,
    pub icon: Option<String>,
    pub class: VentureClass,
    pub retainer_level: u32,
    pub required_stat: u32,
    pub thresholds: Vec<u32>,
    pub quantities: Vec<u32>,
    pub unit_price: Option<f64>,
    pub min_listing: Option<f64>,
    pub average_sale: Option<f64>,
    pub daily_velocity: Option<f64>,
}

/// Les quantités dépendent des stats du servant : le calcul final (palier
/// atteint, gils par mission) se fait dans l'interface.
pub fn price_ventures(ds: &Dataset, markets: &HashMap<u32, Market>) -> Vec<PricedVenture> {
    let empty = Market::default();
    ds.ventures
        .iter()
        .map(|v| {
            let m = markets.get(&v.item).unwrap_or(&empty);
            let (name, icon) = meta(ds, v.item);
            PricedVenture {
                item: v.item,
                name,
                icon,
                class: v.class,
                retainer_level: v.retainer_level,
                required_stat: v.required_stat,
                thresholds: v.thresholds.clone(),
                quantities: v.quantities.clone(),
                unit_price: unit_price(&m.nq),
                min_listing: m.nq.min_listing,
                average_sale: m.nq.average_sale,
                daily_velocity: m.nq.daily_velocity,
            }
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesynthResult {
    pub item: u32,
    pub name: String,
    pub icon: Option<String>,
    pub unit_price: Option<f64>,
    pub daily_velocity: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricedDesynth {
    pub item: u32,
    pub name: String,
    pub icon: Option<String>,
    /// Annonce la moins chère de l'objet à désynthétiser.
    pub buy_price: Option<f64>,
    pub daily_velocity: Option<f64>,
    pub results: Vec<DesynthResult>,
    /// Résultat possible le plus cher (taxe déduite) : un plafond, pas une moyenne.
    pub best_result: Option<f64>,
    /// Meilleur résultat − prix d'achat.
    pub best_gain: Option<f64>,
}

pub fn price_desynths(ds: &Dataset, markets: &HashMap<u32, Market>) -> Vec<PricedDesynth> {
    let empty = Market::default();
    let mut out: Vec<PricedDesynth> = ds
        .desynths
        .iter()
        .map(|d| {
            let m = markets.get(&d.item).unwrap_or(&empty);
            let buy = match (m.nq.min_listing, m.hq.min_listing) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            let results: Vec<DesynthResult> = d
                .results
                .iter()
                .map(|&r| {
                    let rm = markets.get(&r).unwrap_or(&empty);
                    let (name, icon) = meta(ds, r);
                    DesynthResult {
                        item: r,
                        name,
                        icon,
                        unit_price: unit_price(&rm.nq),
                        daily_velocity: rm.nq.daily_velocity,
                    }
                })
                .collect();
            let best = results
                .iter()
                .filter(|r| r.daily_velocity.unwrap_or(0.0) >= MIN_VELOCITY)
                .filter_map(|r| r.unit_price)
                .map(|p| p * (1.0 - MARKET_TAX))
                .reduce(f64::max);
            let (name, icon) = meta(ds, d.item);
            PricedDesynth {
                item: d.item,
                name,
                icon,
                buy_price: buy,
                daily_velocity: velocity(m),
                results,
                best_result: best,
                best_gain: buy.zip(best).map(|(b, r)| r - b),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.best_gain
            .unwrap_or(f64::MIN)
            .total_cmp(&a.best_gain.unwrap_or(f64::MIN))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_price_is_conservative() {
        let q = QualityMarket {
            min_listing: Some(500.0),
            average_sale: Some(300.0),
            ..Default::default()
        };
        assert_eq!(unit_price(&q), Some(300.0));
        let q = QualityMarket {
            last_sale: Some(42.0),
            ..Default::default()
        };
        assert_eq!(unit_price(&q), Some(42.0));
        assert_eq!(unit_price(&QualityMarket::default()), None);
    }

    #[test]
    fn gil_per_currency_accounts_for_quantity_and_cost() {
        use crate::dataset::{ItemMeta, Offer};
        let ds = Dataset {
            version: 1,
            built_at_ms: 0,
            currencies: vec![],
            offers: vec![Offer {
                currency: 41784,
                item: 5057,
                quantity: 3,
                cost: 30,
                hq: false,
                shops: vec![],
                vendors: vec![],
                unlock: None,
            }],
            gil_offers: vec![],
            deliveries: vec![],
            ventures: vec![],
            desynths: vec![],
            items: HashMap::from([(
                5057,
                ItemMeta {
                    name: "Fil".into(),
                    icon: None,
                },
            )]),
        };
        let markets = HashMap::from([(
            5057,
            Market {
                nq: QualityMarket {
                    min_listing: Some(1000.0),
                    ..Default::default()
                },
                ..Default::default()
            },
        )]);
        let rows = price_offers(&ds, 41784, &markets);
        assert_eq!(rows[0].gil_per_currency, Some(100.0)); // 1000 × 3 / 30
    }

    #[test]
    fn gil_offer_profit_deducts_tax_and_npc_price() {
        use crate::dataset::{GilOffer, ItemMeta};
        let ds = Dataset {
            version: 2,
            built_at_ms: 0,
            currencies: vec![],
            offers: vec![],
            gil_offers: vec![GilOffer {
                item: 1,
                price: 100,
                vendors: vec![],
                unlock: None,
            }],
            deliveries: vec![],
            ventures: vec![],
            desynths: vec![],
            items: HashMap::from([(
                1,
                ItemMeta {
                    name: "A".into(),
                    icon: None,
                },
            )]),
        };
        let markets = HashMap::from([(
            1,
            Market {
                nq: QualityMarket {
                    min_listing: Some(1000.0),
                    average_sale: Some(1000.0),
                    daily_velocity: Some(2.0),
                    ..Default::default()
                },
                ..Default::default()
            },
        )]);
        let r = &price_gil_offers(&ds, &markets)[0];
        assert_eq!(r.profit, Some(850.0)); // 1000 × 0,95 − 100
        assert_eq!(r.daily_profit, Some(1700.0));
    }

    #[test]
    fn deliveries_use_cheapest_quality_and_seal_value() {
        use crate::dataset::{Delivery, ItemMeta};
        let ds = Dataset {
            version: 2,
            built_at_ms: 0,
            currencies: vec![],
            offers: vec![],
            gil_offers: vec![],
            deliveries: vec![Delivery {
                item: 1,
                item_level: 600,
                seals: 1000,
            }],
            ventures: vec![],
            desynths: vec![],
            items: HashMap::from([(
                1,
                ItemMeta {
                    name: "A".into(),
                    icon: None,
                },
            )]),
        };
        let markets = HashMap::from([(
            1,
            Market {
                nq: QualityMarket {
                    min_listing: Some(3000.0),
                    ..Default::default()
                },
                hq: QualityMarket {
                    min_listing: Some(2000.0),
                    ..Default::default()
                },
            },
        )]);
        let r = &price_deliveries(&ds, &markets, Some(5.0))[0];
        assert_eq!(r.buy_price, Some(2000.0));
        assert_eq!(r.gil_per_seal, Some(2.0));
        assert_eq!(r.profit, Some(3000.0));
    }
}
