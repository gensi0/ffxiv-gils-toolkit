//! Prix de l'hôtel des ventes via Universalis.

use std::collections::HashMap;

use futures::stream::{self, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};

use crate::http::Http;
use crate::Result;

const BASE: &str = "https://universalis.app/api/v2";
const IDS_PER_REQUEST: usize = 100;

#[derive(Clone)]
pub struct Universalis {
    http: Http,
}

/// Données de marché d'une qualité (NQ ou HQ) sur un serveur.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityMarket {
    /// Annonce la moins chère actuellement en vente.
    pub min_listing: Option<f64>,
    /// Prix moyen des ventes récentes.
    pub average_sale: Option<f64>,
    pub last_sale: Option<f64>,
    pub last_sale_ms: Option<i64>,
    /// Unités vendues par jour.
    pub daily_velocity: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Market {
    pub nq: QualityMarket,
    pub hq: QualityMarket,
}

// --- format de /aggregated ---

#[derive(Deserialize)]
struct AggResponse {
    results: Vec<AggItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AggItem {
    item_id: u32,
    #[serde(default)]
    nq: AggQuality,
    #[serde(default)]
    hq: AggQuality,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct AggQuality {
    min_listing: Levels<Price>,
    recent_purchase: Levels<Price>,
    average_sale_price: Levels<Price>,
    daily_sale_velocity: Levels<Quantity>,
}

#[derive(Deserialize)]
#[serde(default)]
struct Levels<T> {
    world: Option<T>,
}

impl<T> Default for Levels<T> {
    fn default() -> Self {
        Self { world: None }
    }
}

#[derive(Deserialize)]
struct Price {
    price: f64,
    timestamp: Option<i64>,
}

#[derive(Deserialize)]
struct Quantity {
    quantity: f64,
}

impl From<AggQuality> for QualityMarket {
    fn from(q: AggQuality) -> Self {
        QualityMarket {
            min_listing: q.min_listing.world.map(|p| p.price),
            average_sale: q.average_sale_price.world.map(|p| p.price),
            last_sale_ms: q.recent_purchase.world.as_ref().and_then(|p| p.timestamp),
            last_sale: q.recent_purchase.world.map(|p| p.price),
            daily_velocity: q.daily_sale_velocity.world.map(|v| v.quantity),
        }
    }
}

pub fn parse_aggregated(body: &str) -> Result<HashMap<u32, Market>> {
    let r: AggResponse = serde_json::from_str(body)?;
    Ok(r.results
        .into_iter()
        .map(|i| {
            (
                i.item_id,
                Market {
                    nq: i.nq.into(),
                    hq: i.hq.into(),
                },
            )
        })
        .collect())
}

impl Universalis {
    pub fn new(http: Http) -> Self {
        Self { http }
    }

    /// Identifiants des objets vendables à l'hôtel des ventes.
    pub async fn marketable(&self) -> Result<Vec<u32>> {
        self.http.get_json(&format!("{BASE}/marketable")).await
    }

    /// Prix sur un serveur, pour autant d'objets que voulu.
    pub async fn markets(&self, world: &str, ids: &[u32]) -> Result<HashMap<u32, Market>> {
        let urls: Vec<String> = ids
            .chunks(IDS_PER_REQUEST)
            .map(|c| {
                let ids = c.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
                format!("{BASE}/aggregated/{world}/{ids}")
            })
            .collect();
        let parts: Vec<HashMap<u32, Market>> = stream::iter(urls)
            .map(|u| async move {
                let v: serde_json::Value = self.http.get_json(&u).await?;
                parse_aggregated(&v.to_string())
            })
            .buffer_unordered(4)
            .try_collect()
            .await?;
        Ok(parts.into_iter().flatten().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_world_level_only() {
        let body = r#"{"results":[{"itemId":5057,
          "nq":{"minListing":{"world":{"price":100},"dc":{"price":90,"worldId":71}},
                "recentPurchase":{"world":{"price":34,"timestamp":1791725688000}},
                "averageSalePrice":{"world":{"price":53.6}},
                "dailySaleVelocity":{"world":{"quantity":20.9}}},
          "hq":{}}],"failedItems":[]}"#;
        let m = &parse_aggregated(body).unwrap()[&5057];
        assert_eq!(m.nq.min_listing, Some(100.0));
        assert_eq!(m.nq.last_sale_ms, Some(1791725688000));
        assert_eq!(m.nq.daily_velocity, Some(20.9));
        assert_eq!(m.hq, QualityMarket::default());
    }
}
