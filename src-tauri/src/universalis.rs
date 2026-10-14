//! Typed Universalis v2 endpoints on top of [`net::Throttled`] + the SQLite
//! response cache.

use std::collections::HashMap;
use std::time::Duration;

use reqwest::Url;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::net::{self, CachePolicy, Throttled};

const BASE: &str = "https://universalis.app/api/v2";
pub const MAX_IDS_PER_REQUEST: usize = 100;

pub const TTL_MARKET: Duration = Duration::from_secs(10 * 60);
pub const TTL_HISTORY: Duration = Duration::from_secs(60 * 60);
pub const TTL_TAX: Duration = Duration::from_secs(60 * 60);
pub const TTL_MARKETABLE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

// ---------- /aggregated ----------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggResponse {
    pub results: Vec<AggItem>,
    #[serde(default)]
    pub failed_items: Vec<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggItem {
    pub item_id: u32,
    #[serde(default)]
    pub nq: AggQuality,
    #[serde(default)]
    pub hq: AggQuality,
    #[serde(default)]
    pub world_upload_times: Vec<WorldUpload>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AggQuality {
    pub min_listing: Levels<PriceAt>,
    pub recent_purchase: Levels<PriceAt>,
    pub average_sale_price: Levels<PriceAt>,
    pub daily_sale_velocity: Levels<Quantity>,
}

/// A value at world / DC / region level. Levels are absent when the query
/// does not imply them or when Universalis has no recent data.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Levels<T> {
    pub world: Option<T>,
    pub dc: Option<T>,
    pub region: Option<T>,
}

impl<T> Default for Levels<T> {
    fn default() -> Self {
        Self {
            world: None,
            dc: None,
            region: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceAt {
    pub price: f64,
    pub world_id: Option<i64>,
    /// Unix ms.
    pub timestamp: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Quantity {
    pub quantity: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldUpload {
    pub world_id: i64,
    /// Unix ms.
    pub timestamp: i64,
}

// ---------- /{scope}/{ids} ----------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketItem {
    #[serde(rename = "itemID")]
    pub item_id: u32,
    #[serde(rename = "worldID")]
    pub world_id: Option<i64>,
    /// Unix ms; for DC/region queries, the most recent of all worlds.
    #[serde(default)]
    pub last_upload_time: i64,
    #[serde(default)]
    pub listings: Vec<Listing>,
    /// World id (as string) → unix ms; present for DC/region queries.
    #[serde(default)]
    pub world_upload_times: HashMap<String, i64>,
    #[serde(default)]
    pub nq_sale_velocity: f64,
    #[serde(default)]
    pub hq_sale_velocity: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub price_per_unit: u64,
    pub quantity: u32,
    pub hq: bool,
    /// Present for DC/region queries.
    #[serde(rename = "worldID")]
    pub world_id: Option<i64>,
    /// Stable id, used to apply WebSocket `listings/remove` events.
    #[serde(rename = "listingID")]
    pub listing_id: Option<String>,
    /// Nom du servant vendeur, pour reconnaître mes annonces.
    #[serde(default)]
    pub retainer_name: Option<String>,
}

// ---------- /history/{scope}/{ids} ----------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    #[serde(rename = "itemID")]
    pub item_id: u32,
    #[serde(default)]
    pub last_upload_time: i64,
    #[serde(default)]
    pub entries: Vec<Sale>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sale {
    pub hq: bool,
    pub price_per_unit: u64,
    pub quantity: u32,
    /// Unix seconds.
    pub timestamp: i64,
    pub buyer_name: Option<String>,
}

/// Universalis Rust client. Cheap to clone.
#[derive(Clone)]
pub struct Universalis {
    net: Throttled,
    db: SqlitePool,
}

fn ids_csv(ids: &[u32]) -> String {
    ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
}

fn url(path: &str, query: &[(&str, String)]) -> AppResult<Url> {
    Url::parse_with_params(&format!("{BASE}{path}"), query)
        .map_err(|e| AppError::Other(format!("bad URL: {e}")))
}

/// Multi-item responses wrap items in `{"items": {"<id>": …}}`; a single-item
/// request returns the item object itself.
fn parse_per_item<T: DeserializeOwned>(body: &str, ids: &[u32]) -> AppResult<HashMap<u32, T>> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    if ids.len() == 1 && value.get("items").is_none() {
        let item: T = serde_json::from_value(value)?;
        return Ok(HashMap::from([(ids[0], item)]));
    }
    let items = value
        .get("items")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
    let map: HashMap<String, T> = serde_json::from_value(items)?;
    Ok(map
        .into_iter()
        .filter_map(|(k, v)| Some((k.parse().ok()?, v)))
        .collect())
}

impl Universalis {
    pub fn new(net: Throttled, db: SqlitePool) -> Self {
        Self { net, db }
    }

    pub fn net(&self) -> &Throttled {
        &self.net
    }

    async fn get(&self, url: &Url, policy: CachePolicy) -> AppResult<String> {
        if let CachePolicy::MaxAge(max_age) = policy {
            if let Some(body) = net::cache_get(&self.db, url.as_str(), max_age).await? {
                return Ok(body);
            }
        }
        let body = self.net.get_text(url).await?;
        net::cache_put(&self.db, url.as_str(), &body).await?;
        Ok(body)
    }

    /// Uncached: only used by the scan phase, which must read fresh data.
    pub async fn aggregated(&self, scope: &str, ids: &[u32]) -> AppResult<AggResponse> {
        let url = url(&format!("/aggregated/{scope}/{}", ids_csv(ids)), &[])?;
        let body = self.net.get_text(&url).await?;
        Ok(serde_json::from_str(&body)?)
    }

    /// Current listings (cheapest first, at most `listings` per item).
    pub async fn market(
        &self,
        scope: &str,
        ids: &[u32],
        listings: Option<u32>,
        policy: CachePolicy,
    ) -> AppResult<HashMap<u32, MarketItem>> {
        let mut query = vec![("entries", "0".to_string())];
        if let Some(n) = listings {
            query.push(("listings", n.to_string()));
        }
        let url = url(&format!("/{scope}/{}", ids_csv(ids)), &query)?;
        let body = self.get(&url, policy).await?;
        parse_per_item(&body, ids)
    }

    /// Toutes les annonces d'un serveur, réduites aux champs utiles pour
    /// reconnaître les miennes. Non mis en cache : sert au relevé complet.
    pub async fn listings_light(
        &self,
        world: &str,
        ids: &[u32],
    ) -> AppResult<HashMap<u32, MarketItem>> {
        let prefix = if ids.len() == 1 { "" } else { "items." };
        let fields = [
            "itemID",
            "lastUploadTime",
            "listings.listingID",
            "listings.retainerName",
            "listings.pricePerUnit",
            "listings.quantity",
            "listings.hq",
        ]
        .iter()
        .map(|f| format!("{prefix}{f}"))
        .collect::<Vec<_>>()
        .join(",");
        let url = url(
            &format!("/{world}/{}", ids_csv(ids)),
            &[("entries", "0".to_string()), ("fields", fields)],
        )?;
        let body = self.net.get_text(&url).await?;
        parse_per_item(&body, ids)
    }

    /// Sales within the last `within` seconds, newest first.
    pub async fn history(
        &self,
        scope: &str,
        ids: &[u32],
        within: Duration,
        policy: CachePolicy,
    ) -> AppResult<HashMap<u32, HistoryItem>> {
        let url = url(
            &format!("/history/{scope}/{}", ids_csv(ids)),
            &[
                ("entriesWithin", within.as_secs().to_string()),
                ("entriesToReturn", "1800".to_string()),
            ],
        )?;
        let body = self.get(&url, policy).await?;
        parse_per_item(&body, ids)
    }

    pub async fn marketable(&self) -> AppResult<Vec<u32>> {
        let url = url("/marketable", &[])?;
        let body = self.get(&url, CachePolicy::MaxAge(TTL_MARKETABLE)).await?;
        Ok(serde_json::from_str(&body)?)
    }

    /// Seller tax per retainer city for `world`, in percent.
    pub async fn tax_rates(&self, world: &str) -> AppResult<HashMap<String, u8>> {
        let url = url("/tax-rates", &[("world", world.to_string())])?;
        let body = self.get(&url, CachePolicy::MaxAge(TTL_TAX)).await?;
        Ok(serde_json::from_str(&body)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_and_multi_item_market() {
        let single = r#"{"itemID":5,"worldID":80,"lastUploadTime":1,"listings":[
            {"pricePerUnit":10,"quantity":2,"hq":false}]}"#;
        let m: HashMap<u32, MarketItem> = parse_per_item(single, &[5]).unwrap();
        assert_eq!(m[&5].listings[0].price_per_unit, 10);

        let multi = r#"{"itemIDs":[5,6],"items":{
            "5":{"itemID":5,"listings":[]},
            "6":{"itemID":6,"listings":[{"pricePerUnit":3,"quantity":1,"hq":true,"worldID":33}],
                 "worldUploadTimes":{"33":1000}}}}"#;
        let m: HashMap<u32, MarketItem> = parse_per_item(multi, &[5, 6]).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m[&6].listings[0].world_id, Some(33));
        assert_eq!(m[&6].world_upload_times["33"], 1000);
    }

    #[test]
    fn parses_aggregated_with_missing_levels() {
        let body = r#"{"results":[{"itemId":7,"nq":{"minListing":{"dc":{"price":5,"worldId":33}},
            "averageSalePrice":{},"dailySaleVelocity":{}},
            "hq":{},"worldUploadTimes":[{"worldId":33,"timestamp":9}]}],"failedItems":[]}"#;
        let r: AggResponse = serde_json::from_str(body).unwrap();
        let item = &r.results[0];
        assert_eq!(item.nq.min_listing.dc.as_ref().unwrap().world_id, Some(33));
        assert!(item.nq.average_sale_price.world.is_none());
        assert!(item.hq.min_listing.dc.is_none());
    }
}
