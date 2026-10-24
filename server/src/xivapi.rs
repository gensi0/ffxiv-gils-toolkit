//! XIVAPI v2 client: item metadata (cached forever in SQLite), name search,
//! and icons (cached on disk).

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use reqwest::Url;
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::net::now_ms;
use crate::settings::Language;

const BASE: &str = "https://v2.xivapi.com/api";
const ROWS_PER_REQUEST: usize = 100;
/// `ItemSearchCategory.Category` of the market board "Housing" tab.
const HOUSING_CATEGORY: i64 = 4;
const TIMEOUT: Duration = Duration::from_secs(20);

const ITEM_FIELDS: &str = "Name@lang(en),Name@lang(fr),Name@lang(de),Name@lang(ja),Icon,\
StackSize,CanBeHq,LevelItem@as(raw),ItemSearchCategory@as(raw),ItemSearchCategory.Category,\
ItemSearchCategory.Name@lang(en),ItemSearchCategory.Name@lang(fr),\
ItemSearchCategory.Name@lang(de),ItemSearchCategory.Name@lang(ja),ItemUICategory.Name@lang(en)";

/// Cached item metadata, one row of the `items` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ItemRow {
    pub item_id: i64,
    pub name_en: String,
    pub name_fr: Option<String>,
    pub name_de: Option<String>,
    pub name_ja: Option<String>,
    pub icon_path: Option<String>,
    pub category: Option<String>,
    pub category_fr: Option<String>,
    pub category_de: Option<String>,
    pub category_ja: Option<String>,
    pub item_kind: Option<String>,
    pub stack_size: i64,
    pub level_item: Option<i64>,
    pub can_be_hq: bool,
    pub is_marketable: bool,
    /// Market board "Housing" tab; out of scope for this app.
    pub is_housing: bool,
}

/// Item metadata as the UI sees it, localized.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemInfo {
    pub id: u32,
    pub name: String,
    pub icon_path: Option<String>,
    pub category: Option<String>,
    pub stack_size: u32,
    pub level_item: Option<i64>,
    pub can_be_hq: bool,
}

impl ItemRow {
    pub fn localize(&self, lang: Language) -> ItemInfo {
        let pick =
            |en: &Option<String>, fr: &Option<String>, de: &Option<String>, ja: &Option<String>| {
                let localized = match lang {
                    Language::En => en,
                    Language::Fr => fr,
                    Language::De => de,
                    Language::Ja => ja,
                };
                localized
                    .clone()
                    .filter(|s| !s.is_empty())
                    .or_else(|| en.clone())
            };
        ItemInfo {
            id: self.item_id as u32,
            name: pick(
                &Some(self.name_en.clone()),
                &self.name_fr,
                &self.name_de,
                &self.name_ja,
            )
            .unwrap_or_default(),
            icon_path: self.icon_path.clone(),
            category: pick(
                &self.category,
                &self.category_fr,
                &self.category_de,
                &self.category_ja,
            ),
            stack_size: self.stack_size.max(1) as u32,
            level_item: self.level_item,
            can_be_hq: self.can_be_hq,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub id: u32,
    pub name: String,
    pub icon_path: Option<String>,
    pub category: Option<String>,
}

#[derive(Clone)]
pub struct XivApi {
    http: reqwest::Client,
    db: SqlitePool,
    icon_dir: PathBuf,
}

fn str_at<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)?.as_str()
}

fn nested_name(fields: &Value, sheet: &str, lang: &str) -> Option<String> {
    fields
        .get(sheet)?
        .get("fields")?
        .get(format!("Name@lang({lang})"))?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn parse_item_row(row_id: i64, f: &Value) -> ItemRow {
    let name = |lang: &str| str_at(f, &format!("Name@lang({lang})")).map(str::to_owned);
    let category_id = f
        .get("ItemSearchCategory@as(raw)")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    ItemRow {
        item_id: row_id,
        name_en: name("en").unwrap_or_default(),
        name_fr: name("fr"),
        name_de: name("de"),
        name_ja: name("ja"),
        icon_path: f
            .get("Icon")
            .and_then(|i| str_at(i, "path_hr1"))
            .map(str::to_owned),
        category: nested_name(f, "ItemSearchCategory", "en"),
        category_fr: nested_name(f, "ItemSearchCategory", "fr"),
        category_de: nested_name(f, "ItemSearchCategory", "de"),
        category_ja: nested_name(f, "ItemSearchCategory", "ja"),
        item_kind: nested_name(f, "ItemUICategory", "en"),
        stack_size: f.get("StackSize").and_then(Value::as_i64).unwrap_or(1),
        level_item: f.get("LevelItem@as(raw)").and_then(Value::as_i64),
        can_be_hq: f.get("CanBeHq").and_then(Value::as_bool).unwrap_or(false),
        is_marketable: category_id > 0,
        is_housing: f
            .get("ItemSearchCategory")
            .and_then(|c| c.get("fields"))
            .and_then(|c| c.get("Category"))
            .and_then(Value::as_i64)
            == Some(HOUSING_CATEGORY),
    }
}

/// Bundled subset of the game's item sheet (see scripts/gen_item_fallback.py),
/// used only when XIVAPI is unreachable.
const FALLBACK_TSV_GZ: &[u8] = include_bytes!("../data/items-fallback.tsv.gz");

/// `ui/icon/020000/020801_hr1.tex` for icon id 20801.
fn icon_path(icon_id: u32) -> String {
    format!(
        "ui/icon/{:06}/{:06}_hr1.tex",
        icon_id / 1000 * 1000,
        icon_id
    )
}

fn parse_fallback(tsv: &str) -> HashMap<u32, ItemRow> {
    let opt = |s: &str| (!s.is_empty()).then(|| s.to_owned());
    tsv.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            let [id, en, fr, de, ja, icon, stack, hq, level, isc, group, c_en, c_fr, c_de, c_ja] =
                f[..]
            else {
                return None;
            };
            let id: u32 = id.parse().ok()?;
            let icon: u32 = icon.parse().unwrap_or(0);
            Some((
                id,
                ItemRow {
                    item_id: i64::from(id),
                    name_en: en.to_owned(),
                    name_fr: opt(fr),
                    name_de: opt(de),
                    name_ja: opt(ja),
                    icon_path: (icon > 0).then(|| icon_path(icon)),
                    category: opt(c_en),
                    category_fr: opt(c_fr),
                    category_de: opt(c_de),
                    category_ja: opt(c_ja),
                    item_kind: None,
                    stack_size: stack.parse().unwrap_or(1),
                    level_item: level.parse().ok(),
                    can_be_hq: hq == "1",
                    is_marketable: isc.parse::<i64>().unwrap_or(0) > 0,
                    is_housing: group.parse::<i64>().ok() == Some(HOUSING_CATEGORY),
                },
            ))
        })
        .collect()
}

fn fallback_items() -> &'static HashMap<u32, ItemRow> {
    static ITEMS: std::sync::OnceLock<HashMap<u32, ItemRow>> = std::sync::OnceLock::new();
    ITEMS.get_or_init(|| {
        use std::io::Read;
        let mut tsv = String::new();
        match flate2::read::GzDecoder::new(FALLBACK_TSV_GZ).read_to_string(&mut tsv) {
            Ok(_) => parse_fallback(&tsv),
            Err(e) => {
                tracing::error!(error = %e, "bundled item data unreadable");
                HashMap::new()
            }
        }
    })
}

/// Icon paths look like `ui/icon/020000/020801_hr1.tex`.
/// Téléchargements d'icônes simultanés vers XIVAPI (les icônes en cache disque
/// ne passent pas par là).
static ICON_DOWNLOADS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

pub fn is_valid_icon_path(path: &str) -> bool {
    path.starts_with("ui/icon/")
        && path.ends_with(".tex")
        && !path.contains("..")
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '.'))
}

impl XivApi {
    pub fn new(http: reqwest::Client, db: SqlitePool, icon_dir: PathBuf) -> Self {
        Self { http, db, icon_dir }
    }

    async fn get_json(&self, path: &str, query: &[(&str, &str)]) -> AppResult<Value> {
        let url = Url::parse_with_params(&format!("{BASE}{path}"), query)
            .map_err(|e| AppError::Other(format!("bad URL: {e}")))?;
        let res = self
            .http
            .get(url)
            .timeout(TIMEOUT)
            .send()
            .await?
            .error_for_status()?;
        Ok(res.json().await?)
    }

    async fn load_cached(&self, ids: &[u32]) -> AppResult<HashMap<u32, ItemRow>> {
        let mut out = HashMap::new();
        for chunk in ids.chunks(500) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let sql = format!(
                "SELECT item_id, name_en, name_fr, name_de, name_ja, icon_path, category,
                        category_fr, category_de, category_ja, item_kind, stack_size,
                        level_item, can_be_hq, is_marketable, is_housing
                 FROM items WHERE item_id IN ({placeholders})"
            );
            let mut q = sqlx::query_as::<_, ItemRow>(&sql);
            for id in chunk {
                q = q.bind(*id);
            }
            for row in q.fetch_all(&self.db).await? {
                out.insert(row.item_id as u32, row);
            }
        }
        Ok(out)
    }

    async fn store(&self, rows: &[ItemRow]) -> AppResult<()> {
        let mut tx = self.db.begin().await?;
        let now = now_ms();
        for r in rows {
            sqlx::query(
                "INSERT OR REPLACE INTO items (item_id, name_en, name_fr, name_de, name_ja,
                   icon_path, category, category_fr, category_de, category_ja, item_kind,
                   stack_size, level_item, can_be_hq, is_marketable, is_housing, fetched_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(r.item_id)
            .bind(&r.name_en)
            .bind(&r.name_fr)
            .bind(&r.name_de)
            .bind(&r.name_ja)
            .bind(&r.icon_path)
            .bind(&r.category)
            .bind(&r.category_fr)
            .bind(&r.category_de)
            .bind(&r.category_ja)
            .bind(&r.item_kind)
            .bind(r.stack_size)
            .bind(r.level_item)
            .bind(r.can_be_hq)
            .bind(r.is_marketable)
            .bind(r.is_housing)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Metadata for `ids`, fetching (and caching) the ones not yet known.
    /// Items XIVAPI cannot return are simply absent from the result.
    pub async fn items(&self, ids: &[u32]) -> AppResult<HashMap<u32, ItemRow>> {
        let mut known = self.load_cached(ids).await?;
        let missing: Vec<u32> = ids
            .iter()
            .copied()
            .filter(|id| !known.contains_key(id))
            .collect();
        for chunk in missing.chunks(ROWS_PER_REQUEST) {
            let rows_param = chunk
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let body = match self
                .get_json(
                    "/sheet/Item",
                    &[("rows", &rows_param), ("fields", ITEM_FIELDS)],
                )
                .await
            {
                Ok(b) => b,
                Err(e) => {
                    // Serve the bundled subset; it is never cached, so the
                    // live data replaces it once XIVAPI is back.
                    tracing::warn!(error = %e, count = chunk.len(), "XIVAPI item fetch failed, using bundled data");
                    let fallback = fallback_items();
                    known.extend(
                        chunk
                            .iter()
                            .filter_map(|id| Some((*id, fallback.get(id)?.clone()))),
                    );
                    continue;
                }
            };
            let rows: Vec<ItemRow> = body
                .get("rows")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|r| {
                    let id = r.get("row_id")?.as_i64()?;
                    Some(parse_item_row(id, r.get("fields")?))
                })
                .collect();
            self.store(&rows).await?;
            known.extend(rows.into_iter().map(|r| (r.item_id as u32, r)));
        }
        Ok(known)
    }

    /// Marketable items whose name contains `text`, in `lang`.
    pub async fn search(
        &self,
        text: &str,
        lang: Language,
        limit: u32,
    ) -> AppResult<Vec<SearchHit>> {
        let text = text.trim().replace('"', "");
        if text.is_empty() {
            return Ok(Vec::new());
        }
        let lang = serde_json::to_value(lang)?;
        let lang = lang.as_str().unwrap_or("en");
        // Marketable, excluding the housing tab.
        let query = format!(
            "+Name~\"{text}\" +ItemSearchCategory>0 -ItemSearchCategory.Category={HOUSING_CATEGORY}"
        );
        let limit = limit.to_string();
        let body = self
            .get_json(
                "/search",
                &[
                    ("sheets", "Item"),
                    ("query", &query),
                    ("fields", "Name,Icon,ItemSearchCategory.Name"),
                    ("language", lang),
                    ("limit", &limit),
                ],
            )
            .await?;
        Ok(body
            .get("results")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|r| {
                let f = r.get("fields")?;
                Some(SearchHit {
                    id: u32::try_from(r.get("row_id")?.as_i64()?).ok()?,
                    name: str_at(f, "Name")?.to_owned(),
                    icon_path: f
                        .get("Icon")
                        .and_then(|i| str_at(i, "path_hr1"))
                        .map(str::to_owned),
                    category: f
                        .get("ItemSearchCategory")
                        .and_then(|c| c.get("fields"))
                        .and_then(|c| str_at(c, "Name"))
                        .map(str::to_owned),
                })
            })
            .collect())
    }

    /// PNG bytes for an icon path, from the disk cache or XIVAPI.
    pub async fn icon(&self, path: &str) -> AppResult<Vec<u8>> {
        if !is_valid_icon_path(path) {
            return Err(AppError::Invalid(format!("bad icon path {path}")));
        }
        let file = self
            .icon_dir
            .join(format!("{}.png", path.replace('/', "_")));
        if let Ok(bytes) = tokio::fs::read(&file).await {
            return Ok(bytes);
        }
        // Une page affiche des centaines d'icônes : téléchargements limités à
        // quelques-uns en parallèle pour ne pas saturer XIVAPI.
        let _slot = ICON_DOWNLOADS
            .acquire()
            .await
            .expect("sémaphore jamais fermé");
        if let Ok(bytes) = tokio::fs::read(&file).await {
            return Ok(bytes);
        }
        let url = Url::parse_with_params(
            &format!("{BASE}/asset"),
            &[("path", path), ("format", "png")],
        )
        .map_err(|e| AppError::Other(format!("bad URL: {e}")))?;
        let bytes = self
            .http
            .get(url)
            .timeout(TIMEOUT)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?
            .to_vec();
        if let Err(e) = tokio::fs::create_dir_all(&self.icon_dir).await {
            tracing::warn!(error = %e, "cannot create icon cache dir");
        } else if let Err(e) = tokio::fs::write(&file, &bytes).await {
            tracing::warn!(error = %e, "cannot cache icon");
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_item_row() {
        let f: Value = serde_json::from_str(
            r#"{"CanBeHq":true,"Icon":{"id":20801,"path":"ui/icon/020000/020801.tex",
                "path_hr1":"ui/icon/020000/020801_hr1.tex"},
               "ItemSearchCategory":{"value":48,"fields":{"Name@lang(en)":"Metal","Name@lang(fr)":"Métal","Category":3}},
               "ItemSearchCategory@as(raw)":48,"LevelItem@as(raw)":14,
               "Name@lang(en)":"Iron Ingot","Name@lang(fr)":"Lingot de fer","StackSize":999}"#,
        )
        .unwrap();
        let row = parse_item_row(5057, &f);
        assert_eq!(row.name_fr.as_deref(), Some("Lingot de fer"));
        assert_eq!(
            row.icon_path.as_deref(),
            Some("ui/icon/020000/020801_hr1.tex")
        );
        assert_eq!(row.stack_size, 999);
        assert!(row.is_marketable && row.can_be_hq && !row.is_housing);

        let info = row.localize(Language::Fr);
        assert_eq!(
            (info.name.as_str(), info.category.as_deref()),
            ("Lingot de fer", Some("Métal"))
        );
        // Missing German name falls back to English.
        assert_eq!(row.localize(Language::De).name, "Iron Ingot");
    }

    #[test]
    fn bundled_fallback_has_marketable_items() {
        let items = fallback_items();
        assert!(items.len() > 15_000);
        let ingot = &items[&5057];
        assert_eq!(ingot.name_fr.as_deref(), Some("Lingot de fer"));
        assert_eq!(
            ingot.icon_path.as_deref(),
            Some("ui/icon/020000/020801_hr1.tex")
        );
        assert!(ingot.can_be_hq && ingot.is_marketable && !ingot.is_housing);
        assert_eq!(ingot.stack_size, 999);
        assert!(
            items.values().any(|i| i.is_housing),
            "housing flag survives"
        );
    }

    #[test]
    fn icon_path_validation() {
        assert!(is_valid_icon_path("ui/icon/020000/020801_hr1.tex"));
        assert!(!is_valid_icon_path("ui/icon/../../etc/passwd.tex"));
        assert!(!is_valid_icon_path("/etc/hosts"));
    }
}
