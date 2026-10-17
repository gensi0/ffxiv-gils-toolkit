//! World / data-center reference data. Fetched from Universalis `/worlds` +
//! `/data-centers`; a copy of both responses is bundled as a fallback for
//! first launch offline.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::AppResult;

const UNIVERSALIS: &str = "https://universalis.app/api/v2";
const BUNDLED_WORLDS: &str = include_str!("../data/worlds.json");
const BUNDLED_DCS: &str = include_str!("../data/data-centers.json");

/// Regions served by the global game client. The other entries of
/// `/data-centers` (China, Korea, Traditional Chinese, NA Cloud beta) are
/// separate clients a global player can never visit.
const GLOBAL_REGIONS: &[&str] = &["Japan", "North-America", "Europe", "Oceania"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct World {
    #[sqlx(rename = "world_id")]
    pub id: i64,
    pub name: String,
    #[sqlx(rename = "dc_name")]
    pub dc: String,
    pub region: String,
}

#[derive(Deserialize)]
struct ApiWorld {
    id: i64,
    name: String,
}

#[derive(Deserialize)]
struct ApiDataCenter {
    name: String,
    region: String,
    worlds: Vec<i64>,
}

/// Joins the two Universalis lists, keeping global-client worlds only.
fn parse(worlds_json: &str, dcs_json: &str) -> AppResult<Vec<World>> {
    let names: HashMap<i64, String> = serde_json::from_str::<Vec<ApiWorld>>(worlds_json)?
        .into_iter()
        .map(|w| (w.id, w.name))
        .collect();
    let dcs: Vec<ApiDataCenter> = serde_json::from_str(dcs_json)?;
    let mut out: Vec<World> = dcs
        .into_iter()
        .filter(|dc| GLOBAL_REGIONS.contains(&dc.region.as_str()))
        .flat_map(|dc| {
            let names = &names;
            dc.worlds.into_iter().filter_map(move |id| {
                Some(World {
                    id,
                    name: names.get(&id)?.clone(),
                    dc: dc.name.clone(),
                    region: dc.region.clone(),
                })
            })
        })
        .collect();
    out.sort_by(|a, b| (&a.region, &a.dc, &a.name).cmp(&(&b.region, &b.dc, &b.name)));
    Ok(out)
}

pub fn bundled() -> Vec<World> {
    parse(BUNDLED_WORLDS, BUNDLED_DCS).expect("bundled world data is valid")
}

pub async fn fetch(http: &reqwest::Client) -> AppResult<Vec<World>> {
    let get = |path: &'static str| async move {
        http.get(format!("{UNIVERSALIS}{path}"))
            .timeout(Duration::from_secs(15))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await
    };
    let (worlds, dcs) = tokio::try_join!(get("/worlds"), get("/data-centers"))?;
    parse(&worlds, &dcs)
}

/// Replaces the `worlds` table with `worlds`.
pub async fn store(pool: &SqlitePool, worlds: &[World]) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM worlds").execute(&mut *tx).await?;
    for w in worlds {
        sqlx::query("INSERT INTO worlds (world_id, name, dc_name, region) VALUES (?, ?, ?, ?)")
            .bind(w.id)
            .bind(&w.name)
            .bind(&w.dc)
            .bind(&w.region)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn load(pool: &SqlitePool) -> AppResult<Vec<World>> {
    Ok(sqlx::query_as(
        "SELECT world_id, name, dc_name, region FROM worlds ORDER BY region, dc_name, name",
    )
    .fetch_all(pool)
    .await?)
}

/// Seeds the table from the bundled copy when it is empty (first launch).
pub async fn ensure_seeded(pool: &SqlitePool) -> AppResult<()> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM worlds")
        .fetch_one(pool)
        .await?;
    if count == 0 {
        store(pool, &bundled()).await?;
        tracing::info!("worlds seeded from bundled list");
    }
    Ok(())
}

/// Refreshes from Universalis; on failure the existing table is kept.
/// Returns whether the stored list changed.
pub async fn refresh(pool: &SqlitePool, http: &reqwest::Client) -> AppResult<bool> {
    let fresh = fetch(http).await?;
    if fresh.is_empty() {
        return Err(crate::error::AppError::Other(
            "Universalis returned no worlds".into(),
        ));
    }
    if load(pool).await? == fresh {
        tracing::debug!("world list unchanged");
        return Ok(false);
    }
    store(pool, &fresh).await?;
    tracing::info!(count = fresh.len(), "worlds refreshed from Universalis");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn bundled_has_cerberus_on_chaos() {
        let worlds = bundled();
        let w = worlds.iter().find(|w| w.id == 80).expect("Cerberus");
        assert_eq!(
            (w.name.as_str(), w.dc.as_str(), w.region.as_str()),
            ("Cerberus", "Chaos", "Europe")
        );
    }

    #[test]
    fn bundled_excludes_other_clients() {
        assert!(bundled()
            .iter()
            .all(|w| GLOBAL_REGIONS.contains(&w.region.as_str())));
    }

    #[test]
    fn parse_skips_unknown_world_ids() {
        let worlds = r#"[{"id":1,"name":"A"}]"#;
        let dcs = r#"[{"name":"DC","region":"Europe","worlds":[1,2]}]"#;
        let parsed = parse(worlds, dcs).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "A");
    }

    #[tokio::test]
    async fn seed_then_load() {
        let pool = db::open_memory().await;
        ensure_seeded(&pool).await.unwrap();
        assert_eq!(load(&pool).await.unwrap(), bundled());
    }
}
