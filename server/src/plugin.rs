//! Plugin Dalamud : jetons, réception des données du jeu et dépôt de plugins.
//!
//! Le plugin lit (sans rien automatiser) ce que le jeu affiche : annonces du
//! servant ouvert, messages de vente, achats à l'HV, inventaire, monnaies et
//! niveaux. Ces données sont plus fiables que la déduction par Universalis :
//! - annonces : remplacent celles du servant (prix exact, modification de prix
//!   vue tout de suite) ; une annonce absente est rapprochée de l'historique
//!   Universalis, ou passe « à confirmer » ;
//! - vente (message de chat) : notée tout de suite avec le prix de l'annonce ;
//! - achat à l'HV : ajouté au stock (coût FIFO) ;
//! - noms des servants et niveaux d'artisan/récolteur : reportés dans les
//!   paramètres.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine as _;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::app::AppHandle;
use crate::auth::{ApiError, User};
use crate::error::AppResult;
use crate::net::{now_ms, CachePolicy};
use crate::ventes::{self, ListingRow, EVENT_VENTES_UPDATE, LISTING_COLUMNS};
use crate::{scope, settings, worlds, AppState, Server};

pub const EVENT_PLUGIN_UPDATE: &str = "plugin-update";

fn hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

// ---------- jetons ----------

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TokenView {
    id: i64,
    label: String,
    created_at: i64,
    last_used_at: Option<i64>,
}

#[derive(Deserialize)]
pub struct NewToken {
    #[serde(default)]
    label: String,
}

#[derive(Serialize)]
pub struct CreatedToken {
    token: String,
}

pub async fn create_token(
    State(server): State<Arc<Server>>,
    User(app): User,
    Json(t): Json<NewToken>,
) -> Result<Json<CreatedToken>, ApiError> {
    let mut raw = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut raw);
    let token = format!("gtk_{}", B64.encode(raw));
    let label = match t.label.trim() {
        "" => "Plugin Dalamud".to_string(),
        l => l.chars().take(60).collect(),
    };
    sqlx::query(
        "INSERT INTO plugin_tokens (token_hash, user_id, label, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(hash(&token))
    .bind(app.user_id())
    .bind(label)
    .bind(now_ms())
    .execute(&server.shared.accounts)
    .await?;
    Ok(Json(CreatedToken { token }))
}

pub async fn list_tokens(
    State(server): State<Arc<Server>>,
    User(app): User,
) -> Result<Json<Vec<TokenView>>, ApiError> {
    let rows: Vec<TokenView> = sqlx::query_as(
        "SELECT id, label, created_at, last_used_at FROM plugin_tokens
         WHERE user_id = ? ORDER BY created_at DESC",
    )
    .bind(app.user_id())
    .fetch_all(&server.shared.accounts)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct TokenId {
    id: i64,
}

pub async fn revoke_token(
    State(server): State<Arc<Server>>,
    User(app): User,
    Json(t): Json<TokenId>,
) -> Result<StatusCode, ApiError> {
    sqlx::query("DELETE FROM plugin_tokens WHERE id = ? AND user_id = ?")
        .bind(t.id)
        .bind(app.user_id())
        .execute(&server.shared.accounts)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Compte authentifié par le jeton du plugin (`Authorization: Bearer …`).
pub struct PluginUser(pub AppHandle);

impl FromRequestParts<Arc<Server>> for PluginUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, server: &Arc<Server>) -> Result<Self, ApiError> {
        let unauthorized = || ApiError(StatusCode::UNAUTHORIZED, "jeton invalide".into());
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .ok_or_else(unauthorized)?;
        let row: Option<(i64, i64, String)> = sqlx::query_as(
            "SELECT t.id, u.id, u.username FROM plugin_tokens t
             JOIN users u ON u.id = t.user_id WHERE t.token_hash = ?",
        )
        .bind(hash(token))
        .fetch_optional(&server.shared.accounts)
        .await?;
        let Some((token_id, user_id, username)) = row else {
            return Err(unauthorized());
        };
        sqlx::query("UPDATE plugin_tokens SET last_used_at = ? WHERE id = ?")
            .bind(now_ms())
            .bind(token_id)
            .execute(&server.shared.accounts)
            .await?;
        Ok(PluginUser(server.shared.user(user_id, &username).await?))
    }
}

pub async fn ping(PluginUser(app): PluginUser) -> Json<Value> {
    Json(serde_json::json!({ "username": app.username() }))
}

// ---------- données reçues ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterInfo {
    pub name: String,
    pub home_world_id: u32,
    pub current_world_id: u32,
    pub content_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemCount {
    pub item_id: u32,
    pub hq: bool,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetainerInfo {
    pub retainer_id: u64,
    pub name: String,
    pub town: i32,
    pub market_item_count: i32,
    pub market_expire: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetainerListing {
    pub slot: i32,
    pub item_id: u32,
    pub hq: bool,
    pub quantity: i64,
    pub price: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetainerMarket {
    pub retainer_id: u64,
    pub name: String,
    pub listings: Vec<RetainerListing>,
    pub at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetainerInventory {
    pub retainer_id: u64,
    pub name: String,
    pub items: Vec<ItemCount>,
    pub at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameEvent {
    pub kind: String,
    pub id: String,
    pub at: i64,
    #[serde(default)]
    pub item_id: u32,
    #[serde(default)]
    pub hq: bool,
    #[serde(default)]
    pub quantity: i64,
    #[serde(default)]
    pub price_per_unit: i64,
    #[serde(default)]
    pub total_tax: i64,
    #[serde(default)]
    pub world_id: u32,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPayload {
    pub character: Option<CharacterInfo>,
    #[serde(default)]
    pub inventory: Option<Vec<ItemCount>>,
    #[serde(default)]
    pub retainers: Option<Vec<RetainerInfo>>,
    #[serde(default)]
    pub retainer_markets: Option<Vec<RetainerMarket>>,
    #[serde(default)]
    pub retainer_inventories: Option<Vec<RetainerInventory>>,
    #[serde(default)]
    pub job_levels: Option<HashMap<u32, i64>>,
    #[serde(default)]
    pub events: Option<Vec<GameEvent>>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    listings: usize,
    sales: usize,
    purchases: usize,
}

pub async fn sync(
    PluginUser(app): PluginUser,
    Json(p): Json<SyncPayload>,
) -> Result<Json<SyncResult>, ApiError> {
    let res = apply(&app, p).await?;
    let _ = app.emit(EVENT_PLUGIN_UPDATE, ());
    if res.listings + res.sales + res.purchases > 0 {
        let _ = app.emit(EVENT_VENTES_UPDATE, ());
    }
    Ok(Json(res))
}

async fn put_state<T: Serialize>(db: &SqlitePool, key: &str, value: &T) -> AppResult<()> {
    let json =
        serde_json::to_string(value).map_err(|e| crate::error::AppError::Other(e.to_string()))?;
    sqlx::query(
        "INSERT INTO plugin_state (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(json)
    .bind(now_ms())
    .execute(db)
    .await?;
    Ok(())
}

pub async fn apply(app: &AppHandle, p: SyncPayload) -> AppResult<SyncResult> {
    apply_with_state(app.state::<AppState>(), p).await
}

pub async fn apply_with_state(state: &AppState, p: SyncPayload) -> AppResult<SyncResult> {
    let db = &state.db;
    let mut res = SyncResult::default();
    if let Some(c) = &p.character {
        put_state(db, "character", c).await?;
    }
    if let Some(inv) = &p.inventory {
        put_state(db, "inventory", inv).await?;
    }
    if let Some(retainers) = &p.retainers {
        put_state(db, "retainers", retainers).await?;
        let names: Vec<String> = retainers.iter().map(|r| r.name.clone()).collect();
        update_settings(state, Some(&names), None).await?;
    }
    if let Some(levels) = &p.job_levels {
        put_state(db, "job_levels", levels).await?;
        update_settings(state, None, Some(levels)).await?;
    }
    for inv in p.retainer_inventories.iter().flatten() {
        put_state(db, &format!("retainer_inventory:{}", inv.retainer_id), inv).await?;
    }
    for m in p.retainer_markets.iter().flatten() {
        put_state(db, &format!("retainer_market:{}", m.retainer_id), m).await?;
        res.listings += apply_market(state, m).await?;
    }
    for e in p.events.iter().flatten() {
        let inserted = sqlx::query(
            "INSERT INTO plugin_events (id, kind, at, payload, received_at) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(&e.id)
        .bind(&e.kind)
        .bind(e.at)
        .bind(serde_json::to_string(e).unwrap_or_default())
        .bind(now_ms())
        .execute(db)
        .await?
        .rows_affected();
        if inserted == 0 {
            continue;
        }
        match e.kind.as_str() {
            "sale" => {
                if apply_sale(state, e).await? {
                    res.sales += 1;
                }
            }
            "purchase" => {
                if apply_purchase(state, e).await? {
                    res.purchases += 1;
                }
            }
            other => tracing::debug!(kind = other, "événement du plugin ignoré"),
        }
    }
    Ok(res)
}

/// Noms des servants (ajoutés) et niveaux (remplacés) dans les paramètres.
async fn update_settings(
    state: &AppState,
    names: Option<&[String]>,
    levels: Option<&HashMap<u32, i64>>,
) -> AppResult<()> {
    let mut s = settings::load(&state.db).await?;
    let before = serde_json::to_string(&s).unwrap_or_default();
    if let Some(names) = names {
        let known: HashSet<String> = ventes::normalize_names(&s.retainer_names)
            .into_iter()
            .collect();
        for n in names {
            let n = n.trim();
            if !n.is_empty() && !known.contains(&n.to_lowercase()) {
                s.retainer_names.push(n.to_string());
            }
        }
    }
    if let Some(levels) = levels {
        let get = |id: u32| levels.get(&id).copied().unwrap_or(0).clamp(0, 100) as u32;
        let crafters: Vec<u32> = (8..=15).map(get).collect();
        let gatherers: Vec<u32> = (16..=18).map(get).collect();
        if crafters.iter().any(|&l| l > 0) {
            s.crafter_levels = crafters;
        }
        if gatherers.iter().any(|&l| l > 0) {
            s.gatherer_levels = gatherers;
        }
    }
    if serde_json::to_string(&s).unwrap_or_default() != before {
        let worlds = worlds::load(&state.db).await?;
        settings::save(&state.db, &s, &worlds).await?;
    }
    Ok(())
}

/// Annonces d'un servant vues en jeu. Renvoie le nombre d'annonces reçues.
async fn apply_market(state: &AppState, m: &RetainerMarket) -> AppResult<usize> {
    let db = &state.db;
    let rows: Vec<ListingRow> = sqlx::query_as(&format!(
        "SELECT {LISTING_COLUMNS} FROM my_listings
         WHERE status = 'active' AND lower(retainer) = lower(?)"
    ))
    .bind(m.name.trim())
    .fetch_all(db)
    .await?;
    let plan = plan_market(&rows, &m.listings);
    for (row, l) in &plan.matched {
        sqlx::query(
            "UPDATE my_listings SET price = ?, expected_price = NULL,
               last_seen = MAX(last_seen, ?), quantity = ?
             WHERE listing_id = ?",
        )
        .bind(l.price)
        .bind(m.at)
        .bind(l.quantity)
        .bind(&rows[*row].listing_id)
        .execute(db)
        .await?;
    }
    for l in &plan.added {
        sqlx::query(
            "INSERT INTO my_listings
               (listing_id, item_id, hq, price, quantity, retainer, first_seen, last_seen, status)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'active')
             ON CONFLICT(listing_id) DO NOTHING",
        )
        .bind(format!(
            "plugin:{}:{}:{}:{}",
            m.retainer_id, l.slot, l.item_id, m.at
        ))
        .bind(i64::from(l.item_id))
        .bind(l.hq)
        .bind(l.price)
        .bind(l.quantity)
        .bind(m.name.trim())
        .bind(m.at)
        .bind(m.at)
        .execute(db)
        .await?;
    }
    if !plan.gone.is_empty() {
        close_gone(state, plan.gone.iter().map(|&i| &rows[i]).collect(), m.at).await?;
    }
    Ok(m.listings.len())
}

/// Rapprochement des annonces en base (actives, du servant) et des annonces
/// vues en jeu : même objet, qualité et quantité, prix identique en priorité.
#[derive(Debug, Default)]
pub struct MarketPlan<'a> {
    /// (index en base, annonce vue en jeu)
    pub matched: Vec<(usize, &'a RetainerListing)>,
    pub added: Vec<&'a RetainerListing>,
    /// Index en base des annonces disparues.
    pub gone: Vec<usize>,
}

pub fn plan_market<'a>(rows: &[ListingRow], seen: &'a [RetainerListing]) -> MarketPlan<'a> {
    let mut free: Vec<usize> = (0..rows.len()).collect();
    let mut plan = MarketPlan::default();
    // Prix identique d'abord, puis prix modifié.
    let mut pending: Vec<&RetainerListing> = Vec::new();
    for l in seen {
        let pos = free.iter().position(|&i| {
            let r = &rows[i];
            r.item_id == i64::from(l.item_id)
                && r.hq == l.hq
                && r.quantity == l.quantity
                && (r.price == l.price || r.expected_price == Some(l.price))
        });
        match pos {
            Some(p) => plan.matched.push((free.remove(p), l)),
            None => pending.push(l),
        }
    }
    for l in pending {
        let pos = free.iter().position(|&i| {
            let r = &rows[i];
            r.item_id == i64::from(l.item_id) && r.hq == l.hq && r.quantity == l.quantity
        });
        match pos {
            Some(p) => plan.matched.push((free.remove(p), l)),
            None => plan.added.push(l),
        }
    }
    plan.gone = free;
    plan
}

/// Annonces disparues du servant : vente de l'historique Universalis si elle
/// correspond, sinon « à confirmer ».
async fn close_gone(state: &AppState, gone: Vec<&ListingRow>, at: i64) -> AppResult<()> {
    let db = &state.db;
    let mut ids: Vec<u32> = gone.iter().map(|r| r.item_id as u32).collect();
    ids.sort_unstable();
    ids.dedup();
    let settings = settings::load(db).await?;
    let worlds = worlds::load(db).await?;
    let history = match scope::build(&settings, &worlds) {
        Ok(scope) => state
            .universalis
            .history(
                &scope.home.name,
                &ids,
                ventes::HISTORY_WINDOW,
                CachePolicy::MaxAge(std::time::Duration::from_secs(60)),
            )
            .await
            .unwrap_or_default(),
        Err(_) => HashMap::new(),
    };
    let tax = ventes::current_tax(state).await;
    let mut used = HashSet::new();
    for r in gone {
        let sales = history
            .get(&(r.item_id as u32))
            .map(|h| h.entries.as_slice())
            .unwrap_or_default();
        if let Some(sale) = ventes::match_sale(&r.known(), sales, &used) {
            let sold_at = sale.timestamp * 1000;
            used.insert((sold_at, sale.price_per_unit, sale.quantity));
            ventes::record_sale(
                db,
                r.item_id as u32,
                r.hq,
                r.quantity,
                sale.price_per_unit as i64,
                tax,
                sold_at,
                Some(&r.listing_id),
                "auto",
            )
            .await?;
            ventes::close(db, &r.listing_id, "sold", sold_at).await?;
        } else {
            ventes::close(db, &r.listing_id, "pending", at).await?;
        }
    }
    Ok(())
}

/// Montant d'un message de vente (« … pour 12 345 gils … ») : le dernier
/// nombre suivi de « gil ».
pub fn amount_in(text: &str) -> Option<i64> {
    let lower = text.to_lowercase();
    let pos = lower.rfind("gil")?;
    let before: Vec<char> = lower[..pos].chars().collect();
    let mut digits = String::new();
    let mut seen_digit = false;
    for c in before.iter().rev() {
        if c.is_ascii_digit() {
            digits.insert(0, *c);
            seen_digit = true;
        } else if !((seen_digit && matches!(c, ' ' | '\u{a0}' | '\u{202f}' | '.' | ','))
            || (!seen_digit && c.is_whitespace()))
        {
            break;
        }
    }
    digits.parse().ok()
}

/// Message de vente d'un servant : vente notée avec le prix de l'annonce.
async fn apply_sale(state: &AppState, e: &GameEvent) -> AppResult<bool> {
    if e.item_id == 0 {
        return Ok(false);
    }
    let db = &state.db;
    let tax = ventes::current_tax(state).await;
    let listing: Option<ListingRow> = sqlx::query_as(&format!(
        "SELECT {LISTING_COLUMNS} FROM my_listings
         WHERE status = 'active' AND item_id = ? AND hq = ?
         ORDER BY first_seen ASC LIMIT 1"
    ))
    .bind(i64::from(e.item_id))
    .bind(e.hq)
    .fetch_optional(db)
    .await?;
    if let Some(l) = listing {
        let price = l.expected_price.unwrap_or(l.price);
        ventes::record_sale(
            db,
            e.item_id,
            l.hq,
            l.quantity,
            price,
            tax,
            e.at,
            Some(&l.listing_id),
            "plugin",
        )
        .await?;
        ventes::close(db, &l.listing_id, "sold", e.at).await?;
        return Ok(true);
    }
    // Annonce inconnue : montant du message (net de taxe), une unité.
    let Some(net) = e.text.as_deref().and_then(amount_in) else {
        return Ok(false);
    };
    let unit = (net as f64 / (1.0 - tax)).round() as i64;
    ventes::record_sale(db, e.item_id, e.hq, 1, unit, tax, e.at, None, "plugin").await?;
    Ok(true)
}

/// Achat à l'HV : ajouté au stock, taxe d'achat comprise.
async fn apply_purchase(state: &AppState, e: &GameEvent) -> AppResult<bool> {
    if e.item_id == 0 || e.quantity <= 0 || e.price_per_unit <= 0 {
        return Ok(false);
    }
    let unit_cost = e.price_per_unit as f64 + e.total_tax as f64 / e.quantity as f64;
    let world = worlds::load(&state.db)
        .await?
        .into_iter()
        .find(|w| w.id == i64::from(e.world_id))
        .map(|w| w.name);
    ventes::add_purchase(state, e.item_id, e.hq, e.quantity, unit_cost, world).await?;
    Ok(true)
}

// ---------- état pour le client web ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginStatus {
    character: Option<Value>,
    updated_at: Option<i64>,
    /// Quantités possédées (inventaire + servants), par objet.
    owned: HashMap<u32, i64>,
    retainers: Vec<Value>,
}

pub async fn status(app: &AppHandle) -> Result<Value, String> {
    let db = &app.state::<AppState>().db;
    let rows: Vec<(String, String, i64)> =
        sqlx::query_as("SELECT key, value, updated_at FROM plugin_state")
            .fetch_all(db)
            .await
            .map_err(|e| e.to_string())?;
    let mut st = PluginStatus {
        character: None,
        updated_at: rows.iter().map(|r| r.2).max(),
        owned: HashMap::new(),
        retainers: Vec::new(),
    };
    let mut add = |items: &[ItemCount]| {
        for i in items {
            *st.owned.entry(i.item_id).or_default() += i.count;
        }
    };
    for (key, value, _) in &rows {
        let v: Value = serde_json::from_str(value).unwrap_or(Value::Null);
        match key.as_str() {
            "character" => st.character = Some(v),
            "inventory" => {
                if let Ok(items) = serde_json::from_value::<Vec<ItemCount>>(v) {
                    add(&items);
                }
            }
            "retainers" => {
                if let Value::Array(a) = v {
                    st.retainers = a;
                }
            }
            k if k.starts_with("retainer_inventory:") => {
                if let Ok(inv) = serde_json::from_value::<RetainerInventory>(v) {
                    add(&inv.items);
                }
            }
            _ => {}
        }
    }
    serde_json::to_value(st).map_err(|e| e.to_string())
}

// ---------- dépôt de plugins (Dalamud → Paramètres → Dépôts personnalisés) ----------

fn base_url(headers: &HeaderMap) -> String {
    let get = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let host = get("x-forwarded-host")
        .or_else(|| get("host"))
        .unwrap_or("localhost");
    let proto = get("x-forwarded-proto").unwrap_or("https");
    format!("{proto}://{host}")
}

/// `GET /plugin/pluginmaster.json` : le plugin empaqueté dans l'image.
pub async fn pluginmaster(State(server): State<Arc<Server>>, headers: HeaderMap) -> Response {
    let dir = server.web_dir.join("plugin");
    let Ok(text) = std::fs::read_to_string(dir.join("GilsToolkit.json")) else {
        return (StatusCode::NOT_FOUND, "plugin absent de cette image").into_response();
    };
    let Ok(Value::Object(mut manifest)) = serde_json::from_str::<Value>(&text) else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "manifeste illisible").into_response();
    };
    // L'URL change à chaque version : un cache (Cloudflare…) ne peut pas
    // servir l'ancien zip avec le nouveau manifeste (Dalamud refuse alors
    // l'installation).
    let version = manifest
        .get("AssemblyVersion")
        .and_then(Value::as_str)
        .unwrap_or("0")
        .to_string();
    let zip = format!("{}/plugin/latest.zip?v={version}", base_url(&headers));
    let updated = std::fs::metadata(dir.join("latest.zip"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    for (k, v) in [
        ("DownloadLinkInstall", Value::String(zip.clone())),
        ("DownloadLinkUpdate", Value::String(zip.clone())),
        ("DownloadLinkTesting", Value::String(zip)),
        ("IsHide", Value::Bool(false)),
        ("IsTestingExclusive", Value::Bool(false)),
        ("LastUpdate", Value::from(updated)),
        ("DownloadCount", Value::from(0)),
    ] {
        manifest.insert(k.into(), v);
    }
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(Value::Array(vec![Value::Object(manifest)])),
    )
        .into_response()
}

/// `GET /plugin/latest.zip` : jamais en cache (proxy, CDN), sinon un ancien
/// zip peut être servi avec le nouveau manifeste.
pub async fn zip(State(server): State<Arc<Server>>) -> Response {
    match tokio::fs::read(server.web_dir.join("plugin").join("latest.zip")).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "application/zip"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "plugin absent de cette image").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        id: &str,
        item: i64,
        hq: bool,
        price: i64,
        qty: i64,
        expected: Option<i64>,
    ) -> ListingRow {
        ListingRow {
            listing_id: id.into(),
            item_id: item,
            hq,
            price,
            quantity: qty,
            retainer: "Bob".into(),
            first_seen: 0,
            last_seen: 0,
            status: "active".into(),
            expected_price: expected,
            undercut_price: None,
            undercut_notified_price: None,
            rank: None,
            listing_count: None,
            cheapest_other: None,
        }
    }

    fn seen(slot: i32, item: u32, hq: bool, price: i64, qty: i64) -> RetainerListing {
        RetainerListing {
            slot,
            item_id: item,
            hq,
            quantity: qty,
            price,
        }
    }

    #[test]
    fn market_plan_matches_changed_prices_adds_new_and_finds_gone() {
        let rows = vec![
            row("a", 1, false, 100, 10, None),
            row("b", 1, false, 120, 10, None),
            row("c", 2, true, 5000, 1, None),
            row("d", 3, false, 50, 99, None),
        ];
        let seen = vec![
            // b inchangée, a baissée à 90, nouvelle annonce de l'objet 4.
            seen(0, 1, false, 120, 10),
            seen(1, 1, false, 90, 10),
            seen(2, 2, true, 5000, 1),
            seen(3, 4, false, 10, 1),
        ];
        let plan = plan_market(&rows, &seen);
        let matched: Vec<(&str, i64)> = plan
            .matched
            .iter()
            .map(|(i, l)| (rows[*i].listing_id.as_str(), l.price))
            .collect();
        assert!(matched.contains(&("b", 120)));
        assert!(matched.contains(&("a", 90)));
        assert!(matched.contains(&("c", 5000)));
        assert_eq!(plan.added.len(), 1);
        assert_eq!(plan.added[0].item_id, 4);
        assert_eq!(plan.gone, vec![3]);
    }

    #[test]
    fn hq_and_quantity_must_match() {
        let rows = vec![row("a", 1, false, 100, 10, None)];
        let seen = vec![seen(0, 1, true, 100, 10), seen(1, 1, false, 100, 5)];
        let plan = plan_market(&rows, &seen);
        assert!(plan.matched.is_empty());
        assert_eq!(plan.added.len(), 2);
        assert_eq!(plan.gone, vec![0]);
    }

    #[test]
    fn amounts_in_sale_messages() {
        assert_eq!(
            amount_in("Votre servant a vendu une Potion au marché de Limsa Lominsa pour 12 345 gils (taxes déduites)."),
            Some(12345)
        );
        assert_eq!(
            amount_in("The Potion you put up for sale in the Ul'dah markets has sold for 1,234 gil (after fees)."),
            Some(1234)
        );
        assert_eq!(amount_in("pour 9\u{202f}999\u{a0}gils"), Some(9999));
        assert_eq!(amount_in("aucun montant"), None);
    }

    #[tokio::test]
    async fn sync_applies_listings_sales_purchases_and_settings() {
        let db = crate::db::open_memory().await;
        crate::worlds::ensure_seeded(&db).await.unwrap();
        let s = settings::Settings {
            home_world_id: Some(83),
            ..settings::Settings::default()
        };
        settings::save(&db, &s, &crate::worlds::bundled())
            .await
            .unwrap();
        let state = crate::app::test_state(db.clone());

        // Servants, niveaux et annonces du servant « Bob ».
        let p: SyncPayload = serde_json::from_value(serde_json::json!({
            "character": { "name": "Gen Si", "homeWorldId": 83, "currentWorldId": 83, "contentId": 1 },
            "retainers": [{ "retainerId": 7, "name": "Bob", "town": 1, "marketItemCount": 2, "marketExpire": 0 }],
            "jobLevels": { "8": 100, "9": 90, "16": 80 },
            "retainerMarkets": [{ "retainerId": 7, "name": "Bob", "at": 1000, "listings": [
                { "slot": 0, "itemId": 5057, "hq": false, "quantity": 99, "price": 150 },
                { "slot": 1, "itemId": 4551, "hq": true, "quantity": 1, "price": 20000 }
            ]}],
            "inventory": [{ "itemId": 38533, "hq": false, "count": 42 }]
        }))
        .unwrap();
        let r = apply_state(&state, p).await;
        assert_eq!(r.listings, 2);
        let s = settings::load(&db).await.unwrap();
        assert_eq!(s.retainer_names, vec!["Bob".to_string()]);
        assert_eq!(s.crafter_levels[..2], [100, 90]);
        assert_eq!(s.gatherer_levels, vec![80, 0, 0]);

        // Vente de la potion HQ vue dans le chat, puis renvoi du même événement.
        let sale = serde_json::json!({ "events": [{ "kind": "sale", "id": "sale-1", "at": 5000,
            "itemId": 4551, "hq": true, "text": "… pour 19 000 gils" }] });
        let r = apply_state(&state, serde_json::from_value(sale.clone()).unwrap()).await;
        assert_eq!(r.sales, 1);
        let r = apply_state(&state, serde_json::from_value(sale).unwrap()).await;
        assert_eq!(r.sales, 0, "un événement renvoyé n'est compté qu'une fois");
        let sold: Vec<(i64, i64, String)> =
            sqlx::query_as("SELECT unit_price, quantity, source FROM sales_journal")
                .fetch_all(&db)
                .await
                .unwrap();
        assert_eq!(sold, vec![(20000, 1, "plugin".to_string())]);

        // Achat à l'HV : 10 unités à 100 + 50 de taxe → 105 l'unité.
        let buy = serde_json::json!({ "events": [{ "kind": "purchase", "id": "purchase-9", "at": 6000,
            "itemId": 5057, "hq": false, "quantity": 10, "pricePerUnit": 100, "totalTax": 50, "worldId": 80 }] });
        let r = apply_state(&state, serde_json::from_value(buy).unwrap()).await;
        assert_eq!(r.purchases, 1);
        let lot: (i64, f64, Option<String>) =
            sqlx::query_as("SELECT remaining, unit_cost, source_world FROM stock_lots")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(lot, (10, 105.0, Some("Cerberus".to_string())));

        // Nouveau relevé du servant : la potion vendue n'y est plus, le prix
        // des cristaux a baissé.
        let p: SyncPayload = serde_json::from_value(serde_json::json!({
            "retainerMarkets": [{ "retainerId": 7, "name": "Bob", "at": 7000, "listings": [
                { "slot": 0, "itemId": 5057, "hq": false, "quantity": 99, "price": 140 }
            ]}]
        }))
        .unwrap();
        apply_state(&state, p).await;
        let active: Vec<(i64, i64)> =
            sqlx::query_as("SELECT item_id, price FROM my_listings WHERE status = 'active'")
                .fetch_all(&db)
                .await
                .unwrap();
        assert_eq!(active, vec![(5057, 140)]);
    }

    async fn apply_state(state: &AppState, p: SyncPayload) -> SyncResult {
        apply_with_state(state, p).await.unwrap()
    }
}
