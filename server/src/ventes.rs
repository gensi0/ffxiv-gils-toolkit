//! Mes ventes : suivi des annonces de mes servants sur le serveur principal,
//! détection des ventes et journal des gains.
//!
//! Universalis donne le nom du servant de chaque annonce, mais jamais le
//! vendeur d'une vente. Une vente est donc déduite : une de mes annonces
//! disparaît d'un relevé plus récent, et une vente du même objet, même qualité,
//! même prix et même quantité apparaît dans l'historique. Une annonce se vend
//! toujours en entier, ce qui rend le rapprochement fiable. Sans vente
//! correspondante, l'annonce passe « à confirmer » (vendue ou retirée).
//!
//! Le coût d'achat vient du stock (achats notés depuis la liste de courses),
//! consommé dans l'ordre d'achat (FIFO).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use crate::app::AppHandle;
use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::net::{now_ms, CachePolicy};
use crate::results::item_infos;
use crate::settings::{self, Settings};
use crate::universalis::{MarketItem, Sale};
use crate::xivapi::ItemInfo;
use crate::{scan, scope, worlds, AppState};

pub const EVENT_VENTES_UPDATE: &str = "ventes-update";
const POLL_EVERY: Duration = Duration::from_secs(180);
const HISTORY_WINDOW: Duration = Duration::from_secs(3 * 24 * 3600);
/// Tolérance sur les horodatages pour rapprocher une annonce et une vente.
const MATCH_SLACK_MS: i64 = 10 * 60 * 1000;
/// Un relevé Universalis de moins d'une minute est réutilisé.
const MARKET_MAX_AGE: Duration = Duration::from_secs(60);

/// État du suivi propre à un utilisateur (dates des relevés, relevé complet
/// en cours, réveil par le WebSocket).
#[derive(Default)]
pub struct Runtime {
    last_poll_ms: AtomicI64,
    last_discovery_ms: AtomicI64,
    discovering: AtomicBool,
    nudge: tokio::sync::Notify,
}

impl Runtime {
    /// Demande un relevé rapide (annonce ou vente sur un objet suivi).
    pub fn nudge(&self) {
        self.nudge.notify_one();
    }
}

/// Délai laissé à l'API REST d'Universalis pour refléter l'événement.
const NUDGE_DELAY: Duration = Duration::from_secs(15);
/// Relevé complet du serveur (tous les objets) pour trouver toutes mes annonces.
const DISCOVER_EVERY_MS: i64 = 30 * 60 * 1000;
const DISCOVER_CHUNK: usize = 100;

// ---------- logique pure (testée sans réseau) ----------

/// Normalise une liste de noms de servants (casse et espaces ignorés).
pub fn normalize_names(names: &[String]) -> Vec<String> {
    names
        .iter()
        .map(|n| n.trim().to_lowercase())
        .filter(|n| !n.is_empty())
        .collect()
}

pub fn is_mine(retainer: Option<&str>, mine: &[String]) -> bool {
    retainer.is_some_and(|r| mine.iter().any(|m| *m == r.trim().to_lowercase()))
}

/// Annonce connue, telle qu'enregistrée.
#[derive(Debug, Clone, PartialEq)]
pub struct Known {
    pub listing_id: String,
    pub hq: bool,
    pub price: i64,
    pub quantity: i64,
    pub first_seen: i64,
    pub last_seen: i64,
    pub expected_price: Option<i64>,
}

/// Vente de l'historique correspondant à une annonce disparue : même qualité,
/// même quantité, prix affiché ou prix annoncé après modification, postérieure
/// à l'apparition de l'annonce. `used` écarte les ventes déjà attribuées
/// (horodatage ms, prix, quantité).
pub fn match_sale<'a>(
    k: &Known,
    sales: &'a [Sale],
    used: &HashSet<(i64, u64, u32)>,
) -> Option<&'a Sale> {
    let prices: Vec<u64> = std::iter::once(k.price)
        .chain(k.expected_price)
        .filter(|&p| p > 0)
        .map(|p| p as u64)
        .collect();
    let mut found: Vec<&Sale> = sales
        .iter()
        .filter(|s| {
            s.hq == k.hq
                && i64::from(s.quantity) == k.quantity
                && prices.contains(&s.price_per_unit)
                && s.timestamp * 1000 >= k.first_seen - MATCH_SLACK_MS
                && !used.contains(&(s.timestamp * 1000, s.price_per_unit, s.quantity))
        })
        .collect();
    found.sort_by_key(|s| s.timestamp);
    found.first().copied()
}

/// Lot de stock : (id, unités restantes, coût unitaire).
pub type Lot = (i64, i64, f64);

/// Consomme `qty` unités du stock dans l'ordre donné. Renvoie le coût des
/// unités couvertes, leur nombre et les nouveaux restes par lot.
pub fn fifo(lots: &[Lot], qty: i64) -> (f64, i64, Vec<(i64, i64)>) {
    let mut left = qty;
    let mut cost = 0.0;
    let mut updates = Vec::new();
    for &(id, remaining, unit_cost) in lots {
        if left == 0 {
            break;
        }
        let take = remaining.min(left);
        if take <= 0 {
            continue;
        }
        cost += take as f64 * unit_cost;
        left -= take;
        updates.push((id, remaining - take));
    }
    (cost, qty - left, updates)
}

/// Prix conseillé pour repasser devant : un gil sous le concurrent.
pub fn suggested_price(competitor: i64) -> i64 {
    (competitor - 1).max(1)
}

// ---------- vues pour l'interface ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingView {
    pub listing_id: String,
    pub item: ItemInfo,
    pub hq: bool,
    pub price: i64,
    pub quantity: i64,
    pub retainer: String,
    pub first_seen: i64,
    pub last_seen: i64,
    pub status: String,
    pub expected_price: Option<i64>,
    pub undercut_price: Option<i64>,
    pub suggested_price: Option<i64>,
    /// 1 = la moins chère parmi les autres vendeurs de même qualité.
    pub rank: Option<i64>,
    pub listing_count: Option<i64>,
    pub cheapest_other: Option<i64>,
    /// Coût moyen d'achat (stock de la liste de courses), si connu.
    pub unit_cost: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StockView {
    pub item: ItemInfo,
    pub hq: bool,
    pub remaining: i64,
    pub avg_cost: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    /// Faux tant qu'aucun nom de servant n'est renseigné.
    pub configured: bool,
    pub last_poll_ms: Option<i64>,
    pub last_discovery_ms: Option<i64>,
    /// Relevé complet du serveur en cours.
    pub discovering: bool,
    /// Taxe de vente appliquée (0.05 = 5 %).
    pub tax_rate: f64,
    /// Rang visé (paramètres).
    pub top_rank: u32,
    pub listings: Vec<ListingView>,
    /// Annonces disparues sans vente correspondante : vendues ou retirées ?
    pub pending: Vec<ListingView>,
    pub stock: Vec<StockView>,
    /// Objets suivis en plus du stock et des annonces.
    pub tracked: Vec<ItemInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseView {
    pub id: i64,
    pub item: ItemInfo,
    pub hq: bool,
    pub quantity: i64,
    pub remaining: i64,
    pub unit_cost: f64,
    pub bought_at: i64,
    pub source_world: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaleView {
    pub id: i64,
    pub item: ItemInfo,
    pub hq: bool,
    pub quantity: i64,
    pub unit_price: i64,
    pub tax_rate: f64,
    pub sold_at: i64,
    /// Coût des unités couvertes par le stock.
    pub cost: f64,
    pub cost_units: i64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Journal {
    pub purchases: Vec<PurchaseView>,
    pub sales: Vec<SaleView>,
}

// ---------- base de données ----------

#[derive(sqlx::FromRow)]
struct ListingRow {
    listing_id: String,
    item_id: i64,
    hq: bool,
    price: i64,
    quantity: i64,
    retainer: String,
    first_seen: i64,
    last_seen: i64,
    status: String,
    expected_price: Option<i64>,
    undercut_price: Option<i64>,
    undercut_notified_price: Option<i64>,
    rank: Option<i64>,
    listing_count: Option<i64>,
    cheapest_other: Option<i64>,
}

const LISTING_COLUMNS: &str = "listing_id, item_id, hq, price, quantity, retainer, first_seen,
    last_seen, status, expected_price, undercut_price, undercut_notified_price, rank,
    listing_count, cheapest_other";

impl ListingRow {
    fn known(&self) -> Known {
        Known {
            listing_id: self.listing_id.clone(),
            hq: self.hq,
            price: self.price,
            quantity: self.quantity,
            first_seen: self.first_seen,
            last_seen: self.last_seen,
            expected_price: self.expected_price,
        }
    }
}

/// Objets à relever : suivis, en stock, ou avec une annonce active.
async fn tracked_ids(db: &SqlitePool) -> AppResult<Vec<u32>> {
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT item_id FROM tracked_items
         UNION SELECT item_id FROM stock_lots WHERE remaining > 0
         UNION SELECT item_id FROM my_listings WHERE status = 'active'",
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id as u32).collect())
}

/// Enregistre une vente et consomme le stock correspondant (même qualité
/// d'abord, puis l'autre).
#[allow(clippy::too_many_arguments)]
async fn record_sale(
    db: &SqlitePool,
    item_id: u32,
    hq: bool,
    quantity: i64,
    unit_price: i64,
    tax_rate: f64,
    sold_at: i64,
    listing_id: Option<&str>,
    source: &str,
) -> AppResult<()> {
    let mut tx = db.begin().await?;
    let lots: Vec<Lot> = sqlx::query_as(
        "SELECT id, remaining, unit_cost FROM stock_lots
         WHERE item_id = ? AND remaining > 0
         ORDER BY (hq = ?) DESC, bought_at ASC",
    )
    .bind(i64::from(item_id))
    .bind(hq)
    .fetch_all(&mut *tx)
    .await?;
    let (cost, cost_units, updates) = fifo(&lots, quantity);
    for (id, remaining) in updates {
        sqlx::query("UPDATE stock_lots SET remaining = ? WHERE id = ?")
            .bind(remaining)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "INSERT INTO sales_journal
           (item_id, hq, quantity, unit_price, tax_rate, sold_at, cost, cost_units, listing_id, source)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(i64::from(item_id))
    .bind(hq)
    .bind(quantity)
    .bind(unit_price)
    .bind(tax_rate)
    .bind(sold_at)
    .bind(cost)
    .bind(cost_units)
    .bind(listing_id)
    .bind(source)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

// ---------- relevé périodique ----------

struct PollCtx {
    mine: Vec<String>,
    tax_rate: f64,
    notify: bool,
    top_rank: u32,
}

/// Rang d'une annonce parmi les autres vendeurs (même qualité) : 1 + nombre
/// d'annonces strictement moins chères, et nombre total d'annonces.
pub fn rank(my_price: i64, others: &[i64]) -> (i64, i64) {
    let below = others.iter().filter(|&&p| p < my_price).count() as i64;
    (1 + below, others.len() as i64 + 1)
}

/// Enregistre mes annonces d'un relevé. Renvoie les identifiants vus et les
/// nouvelles annonces (qualité, quantité).
async fn upsert_mine(
    db: &SqlitePool,
    item_id: u32,
    market: &MarketItem,
    mine: &[String],
    known_ids: &HashSet<&str>,
) -> AppResult<(HashSet<String>, Vec<(bool, i64)>)> {
    let upload = market.last_upload_time;
    let mut seen = HashSet::new();
    let mut fresh = Vec::new();
    for l in &market.listings {
        if !is_mine(l.retainer_name.as_deref(), mine) {
            continue;
        }
        let Some(id) = l.listing_id.as_deref() else {
            continue;
        };
        if !known_ids.contains(id) {
            fresh.push((l.hq, i64::from(l.quantity)));
        }
        seen.insert(id.to_string());
        sqlx::query(
            "INSERT INTO my_listings
               (listing_id, item_id, hq, price, quantity, retainer, first_seen, last_seen, status)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'active')
             ON CONFLICT(listing_id) DO UPDATE SET
               price = excluded.price,
               quantity = excluded.quantity,
               last_seen = MAX(my_listings.last_seen, excluded.last_seen),
               status = 'active',
               expected_price = CASE WHEN my_listings.expected_price = excluded.price
                                     THEN NULL ELSE my_listings.expected_price END",
        )
        .bind(id)
        .bind(i64::from(item_id))
        .bind(l.hq)
        .bind(l.price_per_unit as i64)
        .bind(i64::from(l.quantity))
        .bind(l.retainer_name.clone().unwrap_or_default())
        .bind(upload)
        .bind(upload)
        .execute(db)
        .await?;
    }
    Ok((seen, fresh))
}

/// Met à jour rang et concurrence de mes annonces actives d'un objet.
/// `notify` à faux marque la situation actuelle comme déjà signalée (relevé
/// complet : pas de rafale de notifications à la découverte).
async fn update_ranks(
    app: &AppHandle,
    db: &SqlitePool,
    ctx: &PollCtx,
    item_id: u32,
    market: &MarketItem,
    notify: bool,
) -> AppResult<()> {
    let mut others: HashMap<bool, Vec<i64>> = HashMap::new();
    for l in &market.listings {
        if !is_mine(l.retainer_name.as_deref(), &ctx.mine) {
            others
                .entry(l.hq)
                .or_default()
                .push(l.price_per_unit as i64);
        }
    }
    let active: Vec<ListingRow> = sqlx::query_as(&format!(
        "SELECT {LISTING_COLUMNS} FROM my_listings WHERE item_id = ? AND status = 'active'"
    ))
    .bind(i64::from(item_id))
    .fetch_all(db)
    .await?;
    for l in &active {
        let mine = l.expected_price.unwrap_or(l.price);
        let competitors = others.get(&l.hq).map(Vec::as_slice).unwrap_or_default();
        let (r, total) = rank(mine, competitors);
        let cheapest = competitors.iter().copied().min();
        let undercut = cheapest.filter(|&c| c < mine);
        sqlx::query(
            "UPDATE my_listings SET undercut_price = ?, rank = ?, listing_count = ?, cheapest_other = ?
             WHERE listing_id = ?",
        )
        .bind(undercut)
        .bind(r)
        .bind(total)
        .bind(cheapest)
        .bind(&l.listing_id)
        .execute(db)
        .await?;
        let out_of_top = r > i64::from(ctx.top_rank);
        if let (true, Some(c)) = (out_of_top, undercut) {
            if l.undercut_notified_price != Some(c) {
                if notify && ctx.notify {
                    notify_undercut(app, item_id, l, c, r).await;
                }
                sqlx::query(
                    "UPDATE my_listings SET undercut_notified_price = ? WHERE listing_id = ?",
                )
                .bind(c)
                .bind(&l.listing_id)
                .execute(db)
                .await?;
            }
        }
    }
    Ok(())
}

/// Applique un relevé Universalis d'un objet.
async fn process_item(
    app: &AppHandle,
    db: &SqlitePool,
    ctx: &PollCtx,
    item_id: u32,
    market: &MarketItem,
    sales: &[Sale],
) -> AppResult<()> {
    let upload = market.last_upload_time;
    if upload <= 0 {
        return Ok(());
    }
    let known: Vec<ListingRow> = sqlx::query_as(&format!(
        "SELECT {LISTING_COLUMNS} FROM my_listings WHERE item_id = ? AND status = 'active'"
    ))
    .bind(i64::from(item_id))
    .fetch_all(db)
    .await?;
    let known_ids: HashSet<&str> = known.iter().map(|k| k.listing_id.as_str()).collect();
    let (seen, mut fresh) = upsert_mine(db, item_id, market, &ctx.mine, &known_ids).await?;

    // Annonces disparues d'un relevé plus récent que leur dernière apparition.
    let used_rows: Vec<(i64, i64, i64)> =
        sqlx::query_as("SELECT sold_at, unit_price, quantity FROM sales_journal WHERE item_id = ?")
            .bind(i64::from(item_id))
            .fetch_all(db)
            .await?;
    let mut used: HashSet<(i64, u64, u32)> = used_rows
        .into_iter()
        .map(|(t, p, q)| (t, p as u64, q as u32))
        .collect();
    for k in known
        .iter()
        .filter(|k| !seen.contains(&k.listing_id) && upload > k.last_seen)
    {
        let known = k.known();
        if let Some(sale) = match_sale(&known, sales, &used) {
            let sold_at = sale.timestamp * 1000;
            used.insert((sold_at, sale.price_per_unit, sale.quantity));
            record_sale(
                db,
                item_id,
                k.hq,
                k.quantity,
                sale.price_per_unit as i64,
                ctx.tax_rate,
                sold_at,
                Some(&k.listing_id),
                "auto",
            )
            .await?;
            close(db, &k.listing_id, "sold", sold_at).await?;
            continue;
        }
        // Une nouvelle annonce de même qualité et quantité : prix modifié en jeu.
        let status = if let Some(pos) = fresh
            .iter()
            .position(|&(hq, q)| hq == k.hq && q == k.quantity)
        {
            fresh.remove(pos);
            "removed"
        } else {
            "pending"
        };
        close(db, &k.listing_id, status, upload).await?;
    }

    update_ranks(app, db, ctx, item_id, market, true).await
}

async fn close(db: &SqlitePool, listing_id: &str, status: &str, at: i64) -> AppResult<()> {
    sqlx::query("UPDATE my_listings SET status = ?, closed_at = ? WHERE listing_id = ?")
        .bind(status)
        .bind(at)
        .bind(listing_id)
        .execute(db)
        .await?;
    Ok(())
}

async fn notify_undercut(
    app: &AppHandle,
    item_id: u32,
    l: &ListingRow,
    competitor: i64,
    rank: i64,
) {
    let state = app.state::<AppState>();
    let lang = settings::load(&state.db)
        .await
        .map(|s| s.language)
        .unwrap_or(settings::Language::Fr);
    let infos = item_infos(&state.xivapi, &[item_id], lang).await;
    let name = infos
        .get(&item_id)
        .map_or_else(|| format!("Objet n°{item_id}"), |i| i.name.clone());
    let body = format!(
        "Tu es {rank}e : quelqu'un vend à {competitor} gils, sous ton prix de {} ({}). Prix conseillé : {}.",
        l.expected_price.unwrap_or(l.price),
        l.retainer,
        suggested_price(competitor)
    );
    if let Err(e) = app.notification().builder().title(&name).body(body).show() {
        tracing::warn!(error = %e, "notification impossible");
    }
}

/// Un relevé complet de tous les objets suivis.
pub async fn poll(app: &AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    let settings: Settings = settings::load(&state.db).await?;
    let mine = normalize_names(&settings.retainer_names);
    if mine.is_empty() {
        return Ok(());
    }
    let worlds = worlds::load(&state.db).await?;
    let Ok(scope) = scope::build(&settings, &worlds) else {
        return Ok(());
    };
    let ids = tracked_ids(&state.db).await?;
    if ids.is_empty() {
        state.ventes.last_poll_ms.store(now_ms(), Ordering::Relaxed);
        return Ok(());
    }
    let ctx = PollCtx {
        mine,
        tax_rate: scan::seller_tax(&state.universalis, &scope, &settings).await,
        notify: settings.notifications_enabled,
        top_rank: settings.sales_top_rank,
    };
    let home = scope.home.name.clone();
    for chunk in ids.chunks(100) {
        let (markets, history) = tokio::join!(
            state
                .universalis
                .market(&home, chunk, None, CachePolicy::MaxAge(MARKET_MAX_AGE)),
            state.universalis.history(
                &home,
                chunk,
                HISTORY_WINDOW,
                CachePolicy::MaxAge(MARKET_MAX_AGE)
            ),
        );
        let markets = markets?;
        let history = history.unwrap_or_default();
        for (&item_id, market) in &markets {
            let sales = history
                .get(&item_id)
                .map(|h| h.entries.as_slice())
                .unwrap_or_default();
            if let Err(e) = process_item(app, &state.db, &ctx, item_id, market, sales).await {
                tracing::warn!(item_id, error = %e, "relevé de mes ventes impossible");
            }
        }
    }
    state.ventes.last_poll_ms.store(now_ms(), Ordering::Relaxed);
    let _ = app.emit(EVENT_VENTES_UPDATE, ());
    Ok(())
}

/// Remet le drapeau de relevé complet à faux, même en cas d'erreur.
struct DiscoveryGuard<'a>(&'a AtomicBool);
impl Drop for DiscoveryGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

/// Relevé complet : lit toutes les annonces du serveur principal (environ
/// 170 requêtes) et enregistre les miennes, qui passent ensuite dans le suivi
/// toutes les 3 minutes. Renvoie le nombre d'annonces trouvées.
pub async fn discover(app: &AppHandle) -> AppResult<usize> {
    let state = app.state::<AppState>();
    if state.ventes.discovering.swap(true, Ordering::Relaxed) {
        return Err(AppError::Invalid("recherche déjà en cours".into()));
    }
    let guard = DiscoveryGuard(&state.ventes.discovering);
    let _ = app.emit(EVENT_VENTES_UPDATE, ());
    let settings: Settings = settings::load(&state.db).await?;
    let mine = normalize_names(&settings.retainer_names);
    if mine.is_empty() {
        return Ok(0);
    }
    let worlds = worlds::load(&state.db).await?;
    let scope = scope::build(&settings, &worlds)?;
    let ctx = PollCtx {
        mine,
        tax_rate: 0.0,
        notify: false,
        top_rank: settings.sales_top_rank,
    };
    let home = scope.home.name.clone();
    let ids = state.universalis.marketable().await?;
    let mut found = 0;
    let mut tasks = tokio::task::JoinSet::new();
    for chunk in ids.chunks(DISCOVER_CHUNK) {
        let api = state.universalis.clone();
        let home = home.clone();
        let chunk = chunk.to_vec();
        tasks.spawn(async move { api.listings_light(&home, &chunk).await });
    }
    let mut failed = 0;
    while let Some(res) = tasks.join_next().await {
        let Ok(Ok(markets)) = res else {
            failed += 1;
            continue;
        };
        for (&item_id, market) in &markets {
            if !market
                .listings
                .iter()
                .any(|l| is_mine(l.retainer_name.as_deref(), &ctx.mine))
            {
                continue;
            }
            let known: Vec<(String,)> = sqlx::query_as(
                "SELECT listing_id FROM my_listings WHERE item_id = ? AND status = 'active'",
            )
            .bind(i64::from(item_id))
            .fetch_all(&state.db)
            .await?;
            let known_ids: HashSet<&str> = known.iter().map(|k| k.0.as_str()).collect();
            let (seen, _) = upsert_mine(&state.db, item_id, market, &ctx.mine, &known_ids).await?;
            found += seen.len();
            update_ranks(app, &state.db, &ctx, item_id, market, false).await?;
        }
    }
    if failed > 0 {
        tracing::warn!(failed, "relevé complet : requêtes échouées");
    }
    state
        .ventes
        .last_discovery_ms
        .store(now_ms(), Ordering::Relaxed);
    drop(guard);
    let _ = app.emit(EVENT_VENTES_UPDATE, ());
    Ok(found)
}

/// Boucle de fond : un relevé toutes les 3 minutes, un relevé complet toutes
/// les 30 minutes.
pub async fn run(app: AppHandle) {
    let state = app.state::<AppState>();
    loop {
        if now_ms() - state.ventes.last_discovery_ms.load(Ordering::Relaxed) >= DISCOVER_EVERY_MS
            && !state.ventes.discovering.load(Ordering::Relaxed)
        {
            match discover(&app).await {
                Ok(n) => tracing::info!(found = n, "relevé complet de mes annonces"),
                Err(e) => tracing::warn!(error = %e, "relevé complet échoué"),
            }
        }
        if let Err(e) = poll(&app).await {
            tracing::warn!(error = %e, "suivi des ventes : relevé échoué");
        }
        // Les objets suivis passent aussi par le WebSocket du suivi.
        if let Ok(ids) = tracked_ids(&state.db).await {
            state.watch.set_ventes(&ids);
        }
        tokio::select! {
            () = tokio::time::sleep(POLL_EVERY) => {}
            () = state.ventes.nudge.notified() => tokio::time::sleep(NUDGE_DELAY).await,
        }
    }
}

// ---------- lecture pour l'interface ----------

fn fallback_item(id: u32) -> ItemInfo {
    ItemInfo {
        id,
        name: format!("Objet n°{id}"),
        icon_path: None,
        category: None,
        stack_size: 1,
        level_item: None,
        can_be_hq: false,
    }
}

async fn infos(
    state: &AppState,
    ids: impl Iterator<Item = u32>,
) -> AppResult<HashMap<u32, ItemInfo>> {
    let mut ids: Vec<u32> = ids.collect();
    ids.sort_unstable();
    ids.dedup();
    let lang = settings::load(&state.db).await?.language;
    Ok(item_infos(&state.xivapi, &ids, lang).await)
}

fn item_of(map: &HashMap<u32, ItemInfo>, id: i64) -> ItemInfo {
    map.get(&(id as u32))
        .cloned()
        .unwrap_or_else(|| fallback_item(id as u32))
}

fn listing_view(
    r: ListingRow,
    map: &HashMap<u32, ItemInfo>,
    costs: &HashMap<(i64, bool), f64>,
) -> ListingView {
    let unit_cost = costs
        .get(&(r.item_id, r.hq))
        .or_else(|| costs.get(&(r.item_id, !r.hq)))
        .copied();
    ListingView {
        unit_cost,
        rank: r.rank,
        listing_count: r.listing_count,
        cheapest_other: r.cheapest_other,
        item: item_of(map, r.item_id),
        suggested_price: r.undercut_price.map(suggested_price),
        listing_id: r.listing_id,
        hq: r.hq,
        price: r.price,
        quantity: r.quantity,
        retainer: r.retainer,
        first_seen: r.first_seen,
        last_seen: r.last_seen,
        status: r.status,
        expected_price: r.expected_price,
        undercut_price: r.undercut_price,
    }
}

pub async fn overview(state: &AppState) -> AppResult<Overview> {
    let settings = settings::load(&state.db).await?;
    let rows: Vec<ListingRow> = sqlx::query_as(&format!(
        "SELECT {LISTING_COLUMNS} FROM my_listings
         WHERE status IN ('active', 'pending') ORDER BY item_id, price"
    ))
    .fetch_all(&state.db)
    .await?;
    let stock: Vec<(i64, bool, i64, f64)> = sqlx::query_as(
        "SELECT item_id, hq, SUM(remaining), SUM(remaining * unit_cost) / SUM(remaining)
         FROM stock_lots WHERE remaining > 0 GROUP BY item_id, hq",
    )
    .fetch_all(&state.db)
    .await?;
    let tracked: Vec<(i64,)> =
        sqlx::query_as("SELECT item_id FROM tracked_items ORDER BY added_at")
            .fetch_all(&state.db)
            .await?;
    let map = infos(
        state,
        rows.iter()
            .map(|r| r.item_id as u32)
            .chain(stock.iter().map(|s| s.0 as u32))
            .chain(tracked.iter().map(|t| t.0 as u32)),
    )
    .await?;
    let (pending, listings): (Vec<ListingRow>, Vec<ListingRow>) =
        rows.into_iter().partition(|r| r.status == "pending");
    let last = state.ventes.last_poll_ms.load(Ordering::Relaxed);
    let last_discovery = state.ventes.last_discovery_ms.load(Ordering::Relaxed);
    let costs: HashMap<(i64, bool), f64> = stock
        .iter()
        .map(|&(id, hq, _, avg)| ((id, hq), avg))
        .collect();
    Ok(Overview {
        configured: !normalize_names(&settings.retainer_names).is_empty(),
        last_poll_ms: (last > 0).then_some(last),
        last_discovery_ms: (last_discovery > 0).then_some(last_discovery),
        discovering: state.ventes.discovering.load(Ordering::Relaxed),
        tax_rate: current_tax(state).await,
        top_rank: settings.sales_top_rank,
        listings: listings
            .into_iter()
            .map(|r| listing_view(r, &map, &costs))
            .collect(),
        pending: pending
            .into_iter()
            .map(|r| listing_view(r, &map, &costs))
            .collect(),
        stock: stock
            .into_iter()
            .map(|(id, hq, remaining, avg)| StockView {
                item: item_of(&map, id),
                hq,
                remaining,
                avg_cost: avg,
            })
            .collect(),
        tracked: tracked.into_iter().map(|(id,)| item_of(&map, id)).collect(),
    })
}

/// Ligne d'achat : id, objet, HQ, quantité, reste, coût unitaire, date, serveur.
type PurchaseRow = (i64, i64, bool, i64, i64, f64, i64, Option<String>);
/// Ligne de vente : id, objet, HQ, quantité, prix, taxe, date, coût, unités couvertes, source.
type SaleRow = (i64, i64, bool, i64, i64, f64, i64, f64, i64, String);

pub async fn journal(state: &AppState, since_ms: i64) -> AppResult<Journal> {
    let purchases: Vec<PurchaseRow> = sqlx::query_as(
        "SELECT id, item_id, hq, quantity, remaining, unit_cost, bought_at, source_world
         FROM stock_lots WHERE bought_at >= ? OR remaining > 0 ORDER BY bought_at DESC",
    )
    .bind(since_ms)
    .fetch_all(&state.db)
    .await?;
    let sales: Vec<SaleRow> = sqlx::query_as(
        "SELECT id, item_id, hq, quantity, unit_price, tax_rate, sold_at, cost, cost_units, source
         FROM sales_journal WHERE sold_at >= ? ORDER BY sold_at DESC",
    )
    .bind(since_ms)
    .fetch_all(&state.db)
    .await?;
    let map = infos(
        state,
        purchases
            .iter()
            .map(|p| p.1 as u32)
            .chain(sales.iter().map(|s| s.1 as u32)),
    )
    .await?;
    Ok(Journal {
        purchases: purchases
            .into_iter()
            .map(|p| PurchaseView {
                id: p.0,
                item: item_of(&map, p.1),
                hq: p.2,
                quantity: p.3,
                remaining: p.4,
                unit_cost: p.5,
                bought_at: p.6,
                source_world: p.7,
            })
            .collect(),
        sales: sales
            .into_iter()
            .map(|s| SaleView {
                id: s.0,
                item: item_of(&map, s.1),
                hq: s.2,
                quantity: s.3,
                unit_price: s.4,
                tax_rate: s.5,
                sold_at: s.6,
                cost: s.7,
                cost_units: s.8,
                source: s.9,
            })
            .collect(),
    })
}

// ---------- actions de l'interface ----------

pub async fn add_purchase(
    state: &AppState,
    item_id: u32,
    hq: bool,
    quantity: i64,
    unit_cost: f64,
    source_world: Option<String>,
) -> AppResult<i64> {
    if quantity <= 0 || !unit_cost.is_finite() || unit_cost < 0.0 {
        return Err(AppError::Invalid("quantité ou prix invalide".into()));
    }
    let id = sqlx::query(
        "INSERT INTO stock_lots (item_id, hq, quantity, remaining, unit_cost, bought_at, source_world)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(i64::from(item_id))
    .bind(hq)
    .bind(quantity)
    .bind(quantity)
    .bind(unit_cost)
    .bind(now_ms())
    .bind(source_world)
    .execute(&state.db)
    .await?
    .last_insert_rowid();
    Ok(id)
}

pub async fn delete_purchase(state: &AppState, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM stock_lots WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(())
}

pub async fn delete_sale(state: &AppState, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM sales_journal WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(())
}

/// Vente saisie à la main (objet vendu hors suivi).
pub async fn add_sale(
    state: &AppState,
    item_id: u32,
    hq: bool,
    quantity: i64,
    unit_price: i64,
) -> AppResult<()> {
    if quantity <= 0 || unit_price < 0 {
        return Err(AppError::Invalid("quantité ou prix invalide".into()));
    }
    let tax = current_tax(state).await;
    record_sale(
        &state.db,
        item_id,
        hq,
        quantity,
        unit_price,
        tax,
        now_ms(),
        None,
        "manual",
    )
    .await
}

async fn current_tax(state: &AppState) -> f64 {
    let Ok(settings) = settings::load(&state.db).await else {
        return 0.05;
    };
    let Ok(worlds) = worlds::load(&state.db).await else {
        return 0.05;
    };
    match scope::build(&settings, &worlds) {
        Ok(scope) => scan::seller_tax(&state.universalis, &scope, &settings).await,
        Err(_) => 0.05,
    }
}

/// Nouveau prix saisi après une modification en jeu, en attendant qu'Universalis
/// le confirme.
pub async fn set_expected_price(state: &AppState, listing_id: &str, price: i64) -> AppResult<()> {
    if price <= 0 {
        return Err(AppError::Invalid("prix invalide".into()));
    }
    sqlx::query(
        "UPDATE my_listings SET expected_price = ?, undercut_notified_price = NULL
         WHERE listing_id = ?",
    )
    .bind(price)
    .bind(listing_id)
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Clôt une annonce à la main : vendue (au prix donné ou affiché) ou retirée.
pub async fn close_listing(
    state: &AppState,
    listing_id: &str,
    sold: bool,
    price: Option<i64>,
) -> AppResult<()> {
    let row: Option<ListingRow> = sqlx::query_as(&format!(
        "SELECT {LISTING_COLUMNS} FROM my_listings WHERE listing_id = ?"
    ))
    .bind(listing_id)
    .fetch_optional(&state.db)
    .await?;
    let Some(row) = row else {
        return Err(AppError::Invalid("annonce introuvable".into()));
    };
    let now = now_ms();
    if sold {
        let unit = price.unwrap_or(row.expected_price.unwrap_or(row.price));
        let tax = current_tax(state).await;
        record_sale(
            &state.db,
            row.item_id as u32,
            row.hq,
            row.quantity,
            unit,
            tax,
            now,
            Some(listing_id),
            "manual",
        )
        .await?;
        close(&state.db, listing_id, "sold", now).await
    } else {
        close(&state.db, listing_id, "removed", now).await
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CraftInput {
    pub item_id: u32,
    pub quantity: f64,
    /// Coût estimé par unité, utilisé pour ce qui n'est pas en stock.
    pub unit_cost: f64,
}

/// Fabrication : consomme les ingrédients en stock (ordre d'achat) et met le
/// résultat en stock à son coût réel. Les ingrédients absents du stock comptent
/// à leur coût estimé. Renvoie le coût unitaire retenu.
pub async fn record_craft(
    state: &AppState,
    item_id: u32,
    quantity: i64,
    ingredients: &[CraftInput],
) -> AppResult<f64> {
    if quantity <= 0 {
        return Err(AppError::Invalid("quantité invalide".into()));
    }
    let mut tx = state.db.begin().await?;
    let mut total = 0.0;
    for ing in ingredients {
        let need = ing.quantity.ceil().max(0.0) as i64;
        let lots: Vec<Lot> = sqlx::query_as(
            "SELECT id, remaining, unit_cost FROM stock_lots
             WHERE item_id = ? AND remaining > 0 ORDER BY bought_at ASC",
        )
        .bind(i64::from(ing.item_id))
        .fetch_all(&mut *tx)
        .await?;
        let (cost, covered, updates) = fifo(&lots, need);
        for (id, remaining) in updates {
            sqlx::query("UPDATE stock_lots SET remaining = ? WHERE id = ?")
                .bind(remaining)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        total += cost + (need - covered) as f64 * ing.unit_cost.max(0.0);
    }
    let unit = total / quantity as f64;
    sqlx::query(
        "INSERT INTO stock_lots (item_id, hq, quantity, remaining, unit_cost, bought_at, source_world)
         VALUES (?, 0, ?, ?, ?, ?, 'Fabriqué')",
    )
    .bind(i64::from(item_id))
    .bind(quantity)
    .bind(quantity)
    .bind(unit)
    .bind(now_ms())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(unit)
}

pub async fn track(state: &AppState, item_id: u32, on: bool) -> AppResult<()> {
    if on {
        sqlx::query("INSERT OR IGNORE INTO tracked_items (item_id, added_at) VALUES (?, ?)")
            .bind(i64::from(item_id))
            .bind(now_ms())
            .execute(&state.db)
            .await?;
    } else {
        sqlx::query("DELETE FROM tracked_items WHERE item_id = ?")
            .bind(i64::from(item_id))
            .execute(&state.db)
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sale(ts_s: i64, price: u64, qty: u32, hq: bool) -> Sale {
        Sale {
            hq,
            price_per_unit: price,
            quantity: qty,
            timestamp: ts_s,
            buyer_name: None,
        }
    }

    fn known(price: i64, qty: i64, first_seen_s: i64) -> Known {
        Known {
            listing_id: "L1".into(),
            hq: false,
            price,
            quantity: qty,
            first_seen: first_seen_s * 1000,
            last_seen: first_seen_s * 1000,
            expected_price: None,
        }
    }

    #[test]
    fn retainer_names_are_case_insensitive() {
        let mine = normalize_names(&["  Mogtaro ".into(), String::new()]);
        assert_eq!(mine, vec!["mogtaro"]);
        assert!(is_mine(Some("MOGTARO"), &mine));
        assert!(!is_mine(Some("Autre"), &mine));
        assert!(!is_mine(None, &mine));
    }

    #[test]
    fn sale_must_match_price_quantity_quality_and_time() {
        let k = known(500, 10, 1_000);
        let used = HashSet::new();
        let sales = [
            sale(100, 500, 10, false), // bien avant l'annonce (hors tolérance de 10 min)
            sale(2_000, 500, 5, false), // mauvaise quantité
            sale(2_000, 450, 10, false), // mauvais prix
            sale(2_000, 500, 10, true), // mauvaise qualité
            sale(3_000, 500, 10, false), // la bonne
        ];
        assert_eq!(
            match_sale(&k, &sales, &used).map(|s| s.timestamp),
            Some(3_000)
        );
        let used = HashSet::from([(3_000_000, 500, 10)]);
        assert!(
            match_sale(&k, &sales, &used).is_none(),
            "vente déjà attribuée"
        );
    }

    #[test]
    fn expected_price_matches_a_sale_after_an_in_game_update() {
        let mut k = known(500, 10, 1_000);
        k.expected_price = Some(480);
        let sales = [sale(2_000, 480, 10, false)];
        assert!(match_sale(&k, &sales, &HashSet::new()).is_some());
    }

    #[test]
    fn fifo_consumes_oldest_lots_and_reports_uncovered_units() {
        let lots = [(1, 3, 100.0), (2, 5, 200.0)];
        let (cost, units, updates) = fifo(&lots, 4);
        assert_eq!(units, 4);
        assert!((cost - (3.0 * 100.0 + 200.0)).abs() < 1e-9);
        assert_eq!(updates, vec![(1, 0), (2, 4)]);
        let (_, units, _) = fifo(&lots, 20);
        assert_eq!(units, 8, "seules 8 unités sont en stock");
    }

    #[test]
    fn rank_counts_strictly_cheaper_competitors() {
        assert_eq!(rank(500, &[]), (1, 1));
        assert_eq!(
            rank(500, &[400, 500, 600]),
            (2, 4),
            "égalité : pas devant moi"
        );
        assert_eq!(rank(500, &[100, 200, 300]), (4, 4));
    }

    #[test]
    fn suggested_price_undercuts_by_one() {
        assert_eq!(suggested_price(1_000), 999);
        assert_eq!(suggested_price(1), 1);
    }
}
