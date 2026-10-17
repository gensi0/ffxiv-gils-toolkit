//! Outils « dépenser » (ex-FFXIV Spender) : monnaies, revente PNJ, livraisons
//! de grande compagnie, servants, désynthèse. La logique vit dans la crate
//! `spender-core` ; ici, l'état Tauri, le cache disque et les commandes.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use spender_core::dataset::{self, Currency, Dataset, DATASET_VERSION};
use spender_core::http::Http;
use spender_core::pricing::{
    self, CurrencyValue, PricedDelivery, PricedDesynth, PricedGilOffer, PricedOffer, PricedVenture,
};
use spender_core::universalis::{self, Market};

use crate::app::{AppHandle, State};
use crate::error::AppResult;
use crate::net::CachePolicy;
use crate::universalis::MAX_IDS_PER_REQUEST;
use crate::{settings, AppState};
use tokio::sync::{Mutex, RwLock};

/// Les prix sont relus au plus toutes les 10 minutes, sauf demande explicite.
const PRICE_TTL: Duration = Duration::from_secs(10 * 60);

/// Clé de cache : (serveur, liste d'objets). La liste est désignée par un nom
/// (« currency:20 », « gil », « deliveries »).
type PriceCache = HashMap<(String, String), (Instant, Arc<HashMap<u32, Market>>)>;

/// Sceaux de grande compagnie : Maelstrom, Deux Vipères, Immortels.
const GC_SEALS: [u32; 3] = [20, 21, 22];

pub struct SpenderState {
    http: Http,
    data_file: PathBuf,
    dataset: RwLock<Option<Arc<Dataset>>>,
    building: Mutex<()>,
    prices: Mutex<PriceCache>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    ready: bool,
    built_at_ms: Option<i64>,
}

impl SpenderState {
    /// Charge le jeu de données déjà construit, s'il existe et est à jour.
    pub fn new(data_file: PathBuf) -> Self {
        let dataset = load_dataset(&data_file).map(Arc::new);
        tracing::info!(path = %data_file.display(), ready = dataset.is_some(), "données des outils de dépense");
        Self {
            http: Http::new(),
            data_file,
            dataset: RwLock::new(dataset),
            building: Mutex::new(()),
            prices: Mutex::new(HashMap::new()),
        }
    }
}

type CmdResult<T> = Result<T, String>;

fn load_dataset(path: &PathBuf) -> Option<Dataset> {
    let bytes = std::fs::read(path).ok()?;
    let ds: Dataset = serde_json::from_slice(&bytes)
        .map_err(|e| tracing::warn!(error = %e, "jeu de données illisible, reconstruction"))
        .ok()?;
    (ds.version == DATASET_VERSION).then_some(ds)
}

impl SpenderState {
    /// Jeu de données déjà construit (marchands PNJ, monnaies…), s'il existe.
    pub async fn dataset(&self) -> Option<Arc<Dataset>> {
        self.dataset.read().await.clone()
    }
}

async fn current(state: &SpenderState) -> CmdResult<Arc<Dataset>> {
    state
        .dataset
        .read()
        .await
        .clone()
        .ok_or_else(|| "Les données du jeu ne sont pas encore prêtes.".to_string())
}

pub async fn spender_dataset_status(state: State<'_, SpenderState>) -> CmdResult<Status> {
    let ds = state.dataset.read().await;
    Ok(Status {
        ready: ds.is_some(),
        built_at_ms: ds.as_ref().map(|d| d.built_at_ms),
    })
}

/// (Re)construit le jeu de données depuis XIVAPI et l'enregistre sur disque.
/// Émet `build-progress` à chaque étape.
pub async fn spender_build_dataset(
    app: AppHandle,
    state: State<'_, SpenderState>,
) -> CmdResult<Status> {
    let _guard = state.building.lock().await;
    let emitter = app.clone();
    let ds = dataset::build(&state.http, &move |p| {
        let _ = emitter.emit("build-progress", p);
    })
    .await
    .map_err(|e| e.to_string())?;
    if let Some(dir) = state.data_file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let bytes = serde_json::to_vec(&ds).map_err(|e| e.to_string())?;
    std::fs::write(&state.data_file, bytes).map_err(|e| e.to_string())?;
    let built_at_ms = ds.built_at_ms;
    *state.dataset.write().await = Some(Arc::new(ds));
    state.prices.lock().await.clear();
    Ok(Status {
        ready: true,
        built_at_ms: Some(built_at_ms),
    })
}

pub async fn spender_currencies(state: State<'_, SpenderState>) -> CmdResult<Vec<Currency>> {
    Ok(current(state).await?.currencies.clone())
}

/// Taxe de vente de la semaine sur `world`, pour la cité de tes servants
/// (5 % si Universalis ne répond pas).
async fn seller_tax_for(app: &AppState, world: &str) -> f64 {
    let Ok(s) = settings::load(&app.db).await else {
        return pricing::MARKET_TAX;
    };
    let city = serde_json::to_value(s.retainer_city)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    match app.universalis.tax_rates(world).await {
        Ok(rates) => rates
            .get(&city)
            .map_or(pricing::MARKET_TAX, |&p| f64::from(p) / 100.0),
        Err(_) => pricing::MARKET_TAX,
    }
}

/// Taxe de vente appliquée aux calculs des outils de dépense.
pub async fn spender_tax(app: State<'_, AppState>, world: String) -> CmdResult<f64> {
    check_world(&world)?;
    Ok(seller_tax_for(app, &world).await)
}

/// Le nom de serveur finit dans une URL Universalis : lettres uniquement.
fn check_world(world: &str) -> CmdResult<()> {
    if !world.is_empty() && world.chars().all(|c| c.is_ascii_alphabetic()) {
        Ok(())
    } else {
        Err(format!("Serveur inconnu : {world}"))
    }
}

/// Prix d'une liste d'objets sur un serveur, avec cache de [`PRICE_TTL`].
async fn markets(
    app: &AppState,
    state: &SpenderState,
    world: &str,
    list: String,
    ids: impl Iterator<Item = u32>,
    refresh: bool,
) -> CmdResult<Arc<HashMap<u32, Market>>> {
    let key = (world.to_string(), list);
    if !refresh {
        let cache = state.prices.lock().await;
        if let Some((at, m)) = cache.get(&key) {
            if at.elapsed() < PRICE_TTL {
                return Ok(m.clone());
            }
        }
    }
    let mut ids: Vec<u32> = ids.collect();
    ids.sort_unstable();
    ids.dedup();
    // Même client que le reste de l'app : limite de débit partagée et cache disque.
    let policy = if refresh {
        CachePolicy::Refresh
    } else {
        CachePolicy::MaxAge(PRICE_TTL)
    };
    let mut tasks = tokio::task::JoinSet::new();
    for chunk in ids.chunks(MAX_IDS_PER_REQUEST) {
        let api = app.universalis.clone();
        let (world, chunk) = (world.to_string(), chunk.to_vec());
        tasks.spawn(async move { api.aggregated_raw(&world, &chunk, policy).await });
    }
    let mut all = HashMap::new();
    while let Some(res) = tasks.join_next().await {
        let body: AppResult<String> = res.map_err(|e| e.to_string())?;
        let body = body.map_err(|e| e.to_string())?;
        all.extend(universalis::parse_aggregated(&body).map_err(|e| e.to_string())?);
    }
    let m = Arc::new(all);
    state
        .prices
        .lock()
        .await
        .insert(key, (Instant::now(), m.clone()));
    Ok(m)
}

async fn currency_markets(
    app: &AppState,
    state: &SpenderState,
    ds: &Dataset,
    world: &str,
    currency: u32,
    refresh: bool,
) -> CmdResult<Arc<HashMap<u32, Market>>> {
    let ids = ds
        .offers
        .iter()
        .filter(|o| o.currency == currency)
        .map(|o| o.item);
    markets(
        app,
        state,
        world,
        format!("currency:{currency}"),
        ids,
        refresh,
    )
    .await
}

/// Échanges d'une monnaie, chiffrés aux prix du serveur choisi.
pub async fn spender_offers(
    app: State<'_, AppState>,
    state: State<'_, SpenderState>,
    currency: u32,
    world: String,
    refresh: bool,
) -> CmdResult<Vec<PricedOffer>> {
    check_world(&world)?;
    let ds = current(state).await?;
    let m = currency_markets(app, state, &ds, &world, currency, refresh).await?;
    Ok(pricing::price_offers(&ds, currency, &m))
}

/// Objets des marchands PNJ, chiffrés à la revente.
pub async fn spender_gil_offers(
    app: State<'_, AppState>,
    state: State<'_, SpenderState>,
    world: String,
    refresh: bool,
) -> CmdResult<Vec<PricedGilOffer>> {
    check_world(&world)?;
    let ds = current(state).await?;
    let ids = ds.gil_offers.iter().map(|o| o.item);
    let m = markets(app, state, &world, "gil".into(), ids, refresh).await?;
    let tax = seller_tax_for(app, &world).await;
    Ok(pricing::price_gil_offers(&ds, &m, tax))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Deliveries {
    seal_value: Option<CurrencyValue>,
    rows: Vec<PricedDelivery>,
}

/// Livraisons expertes, avec la valeur du sceau de la compagnie choisie.
pub async fn spender_deliveries(
    app: State<'_, AppState>,
    state: State<'_, SpenderState>,
    world: String,
    seal: u32,
    refresh: bool,
) -> CmdResult<Deliveries> {
    check_world(&world)?;
    if !GC_SEALS.contains(&seal) {
        return Err(format!("Sceau inconnu : {seal}"));
    }
    let ds = current(state).await?;
    let seal_markets = currency_markets(app, state, &ds, &world, seal, refresh).await?;
    let tax = seller_tax_for(app, &world).await;
    let seal_value = pricing::currency_value(&ds, seal, &seal_markets, tax);
    let ids = ds.deliveries.iter().map(|d| d.item);
    let m = markets(app, state, &world, "deliveries".into(), ids, refresh).await?;
    let rows = pricing::price_deliveries(&ds, &m, seal_value.as_ref().map(|v| v.gil));
    Ok(Deliveries { seal_value, rows })
}

/// Missions de servants ciblées, avec les prix des objets rapportés.
pub async fn spender_ventures(
    app: State<'_, AppState>,
    state: State<'_, SpenderState>,
    world: String,
    refresh: bool,
) -> CmdResult<Vec<PricedVenture>> {
    check_world(&world)?;
    let ds = current(state).await?;
    let ids = ds.ventures.iter().map(|v| v.item);
    let m = markets(app, state, &world, "ventures".into(), ids, refresh).await?;
    Ok(pricing::price_ventures(&ds, &m))
}

/// Objets à désynthétiser, avec la valeur de leurs résultats possibles.
pub async fn spender_desynths(
    app: State<'_, AppState>,
    state: State<'_, SpenderState>,
    world: String,
    refresh: bool,
) -> CmdResult<Vec<PricedDesynth>> {
    check_world(&world)?;
    let ds = current(state).await?;
    let ids = ds
        .desynths
        .iter()
        .flat_map(|d| std::iter::once(d.item).chain(d.results.iter().copied()));
    let m = markets(app, state, &world, "desynths".into(), ids, refresh).await?;
    let tax = seller_tax_for(app, &world).await;
    Ok(pricing::price_desynths(&ds, &m, tax))
}
