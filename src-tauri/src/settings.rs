//! User settings, persisted as one JSON-encoded value per key in the
//! `settings` table. Missing keys fall back to defaults, so adding a field
//! never needs a migration.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::worlds::World;

/// City where the selling retainers are stationed; sets the seller tax.
/// Serialized names match the keys of Universalis `/tax-rates`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetainerCity {
    #[serde(rename = "Limsa Lominsa")]
    LimsaLominsa,
    #[serde(rename = "Gridania")]
    Gridania,
    #[serde(rename = "Ul'dah")]
    Uldah,
    #[serde(rename = "Ishgard")]
    Ishgard,
    #[serde(rename = "Kugane")]
    Kugane,
    #[serde(rename = "Crystarium")]
    Crystarium,
    #[serde(rename = "Old Sharlayan")]
    OldSharlayan,
    #[serde(rename = "Tuliyollal")]
    Tuliyollal,
}

/// Item name language; values match XIVAPI's `language` parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    En,
    Fr,
    De,
    Ja,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Dark,
    Light,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// `None` until the first-launch setup is done.
    pub home_world_id: Option<i64>,
    pub retainer_city: RetainerCity,
    pub language: Language,
    /// Source regions allowed in addition to the home world's own region.
    pub extra_regions: Vec<String>,
    /// Player time for one round trip, used for profit per hour.
    pub travel_minutes_in_region: u32,
    pub travel_minutes_cross_region: u32,
    /// Minimum expected profit per trip, in gil.
    pub min_profit_per_trip: i64,
    /// Minimum units sold per day on the home world.
    pub min_units_per_day: f64,
    /// How many days the player accepts holding stock.
    pub sell_horizon_days: u32,
    /// Worlds (source and home) whose last upload is older are ignored.
    pub max_data_age_hours: u32,
    /// Scan prefilter: minimum estimated profit / cost.
    pub min_margin: f64,
    /// Scan: how many prefiltered items get the full drill-down.
    pub drill_down_top_n: u32,
    /// Automatic re-scan interval; 0 disables it.
    pub scan_interval_hours: u32,
    /// Desktop notification when a watched item's profit per trip jumps.
    pub notifications_enabled: bool,
    /// Default notification threshold (profit per trip, gil) for watched items.
    pub watch_notify_profit: i64,
    pub theme: Theme,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            home_world_id: None,
            retainer_city: RetainerCity::LimsaLominsa,
            language: Language::Fr,
            extra_regions: Vec::new(),
            travel_minutes_in_region: 10,
            travel_minutes_cross_region: 20,
            min_profit_per_trip: 10_000,
            min_units_per_day: 1.0,
            sell_horizon_days: 3,
            max_data_age_hours: 24,
            min_margin: 0.20,
            drill_down_top_n: 200,
            scan_interval_hours: 0,
            notifications_enabled: true,
            watch_notify_profit: 100_000,
            theme: Theme::Dark,
        }
    }
}

impl Settings {
    /// Checks the settings against the current world list.
    pub fn validate(&self, worlds: &[World]) -> AppResult<()> {
        let invalid = |msg: String| Err(AppError::Invalid(msg));
        if let Some(id) = self.home_world_id {
            let Some(home) = worlds.iter().find(|w| w.id == id) else {
                return invalid(format!("unknown world id {id}"));
            };
            for region in &self.extra_regions {
                if *region == home.region {
                    return invalid(format!("{region} is already the home region"));
                }
                if !worlds.iter().any(|w| w.region == *region) {
                    return invalid(format!("unknown region {region}"));
                }
            }
        }
        if self.travel_minutes_in_region == 0 || self.travel_minutes_cross_region == 0 {
            return invalid("travel time must be at least 1 minute".into());
        }
        if self.min_profit_per_trip < 0 {
            return invalid("minimum profit must be >= 0".into());
        }
        if !self.min_units_per_day.is_finite() || self.min_units_per_day < 0.0 {
            return invalid("minimum sell rate must be >= 0".into());
        }
        if self.sell_horizon_days == 0 {
            return invalid("sell horizon must be at least 1 day".into());
        }
        if !self.min_margin.is_finite() || self.min_margin < 0.0 {
            return invalid("minimum margin must be >= 0".into());
        }
        if !(1..=2000).contains(&self.drill_down_top_n) {
            return invalid("drill-down size must be between 1 and 2000".into());
        }
        if self.watch_notify_profit < 0 {
            return invalid("notification threshold must be >= 0".into());
        }
        if self.max_data_age_hours == 0 {
            return invalid("data freshness limit must be at least 1 hour".into());
        }
        Ok(())
    }
}

pub async fn load(pool: &SqlitePool) -> AppResult<Settings> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(pool)
        .await?;
    let mut map = serde_json::Map::new();
    for (key, value) in rows {
        match serde_json::from_str(&value) {
            Ok(v) => {
                map.insert(key, v);
            }
            Err(e) => tracing::warn!(%key, error = %e, "ignoring unreadable setting"),
        }
    }
    Ok(
        serde_json::from_value(serde_json::Value::Object(map)).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "settings unreadable, using defaults");
            Settings::default()
        }),
    )
}

pub async fn save(pool: &SqlitePool, settings: &Settings, worlds: &[World]) -> AppResult<()> {
    settings.validate(worlds)?;
    let serde_json::Value::Object(map) = serde_json::to_value(settings)? else {
        unreachable!("Settings serializes to an object");
    };
    let mut tx = pool.begin().await?;
    for (key, value) in map {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value.to_string())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, worlds};

    fn chaos_home() -> Settings {
        Settings {
            home_world_id: Some(80),
            ..Settings::default()
        }
    }

    #[tokio::test]
    async fn defaults_when_empty() {
        let pool = db::open_memory().await;
        assert_eq!(load(&pool).await.unwrap(), Settings::default());
    }

    #[tokio::test]
    async fn round_trip() {
        let pool = db::open_memory().await;
        let s = Settings {
            retainer_city: RetainerCity::Kugane,
            language: Language::En,
            extra_regions: vec!["Oceania".into()],
            travel_minutes_in_region: 8,
            travel_minutes_cross_region: 25,
            min_profit_per_trip: 25_000,
            min_units_per_day: 2.5,
            sell_horizon_days: 5,
            max_data_age_hours: 12,
            min_margin: 0.3,
            drill_down_top_n: 150,
            scan_interval_hours: 6,
            notifications_enabled: false,
            watch_notify_profit: 250_000,
            theme: Theme::Light,
            ..chaos_home()
        };
        save(&pool, &s, &worlds::bundled()).await.unwrap();
        assert_eq!(load(&pool).await.unwrap(), s);
    }

    #[test]
    fn rejects_home_region_as_extra() {
        let s = Settings {
            extra_regions: vec!["Europe".into()],
            ..chaos_home()
        };
        assert!(s.validate(&worlds::bundled()).is_err());
    }

    #[test]
    fn rejects_unknown_region_and_world() {
        let worlds = worlds::bundled();
        let s = Settings {
            extra_regions: vec!["Atlantis".into()],
            ..chaos_home()
        };
        assert!(s.validate(&worlds).is_err());
        let s = Settings {
            home_world_id: Some(999_999),
            ..Settings::default()
        };
        assert!(s.validate(&worlds).is_err());
    }

    #[test]
    fn rejects_zero_horizon() {
        let s = Settings {
            sell_horizon_days: 0,
            ..chaos_home()
        };
        assert!(s.validate(&worlds::bundled()).is_err());
    }

    #[test]
    fn names_match_external_apis() {
        assert_eq!(
            serde_json::to_string(&RetainerCity::Uldah).unwrap(),
            "\"Ul'dah\""
        );
        assert_eq!(serde_json::to_string(&Language::Fr).unwrap(), "\"fr\"");
    }
}
