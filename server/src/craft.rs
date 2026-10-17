//! Artisanat : objets rentables à fabriquer. Coût des composants au moins cher
//! dans le périmètre d'achat (achat ou fabrication), vente sur le serveur
//! principal. Données agrégées Universalis, comme la présélection du scan.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use crate::app::{AppHandle, State};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use spender_core::craft::{
    self, BuyPrice, Costing, Gatherable, LodestoneJobs, Node, Origin, Recipe, Source,
};
use spender_core::http::Http;
use spender_core::xivapi::XivApi as CoreXivApi;
use tokio::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::net::CachePolicy;
use crate::net::{now_ms, MAX_IN_FLIGHT};
use crate::results::item_infos;
use crate::scan::{home_estimate, seller_tax, upload_of};
use crate::settings::{self, Settings};
use crate::spender::SpenderState;
use crate::universalis::{AggItem, AggQuality, MAX_IDS_PER_REQUEST, TTL_MARKET};
use crate::xivapi::ItemInfo;
use crate::{scope, worlds, AppState};

pub const EVENT_CRAFT_PROGRESS: &str = "craft-progress";
/// Recettes relues sur XIVAPI au-delà de cet âge (nouveaux patchs).
const RECIPES_MAX_AGE_MS: i64 = 7 * 24 * 3600 * 1000;
const MAX_ROWS: usize = 300;
/// Résultats recalculés avec les vraies annonces (quantités comprises).
const REAL_PRICED_ROWS: usize = 40;
const BUYER_TAX: f64 = 0.05;

#[derive(Serialize, Deserialize)]
struct RecipeFile {
    fetched_at_ms: i64,
    recipes: Vec<Recipe>,
    #[serde(default)]
    gatherables: Vec<Gatherable>,
}

/// Recettes et récoltables, relus ensemble chaque semaine.
pub struct GameData {
    pub recipes: Vec<Recipe>,
    pub gatherables: Vec<Gatherable>,
}

pub struct CraftState {
    http: Http,
    file: PathBuf,
    recipes: Mutex<Option<(i64, Arc<GameData>)>>,
    running: Mutex<()>,
    last: Mutex<Option<CraftResults>>,
}

impl CraftState {
    pub fn new(file: PathBuf) -> Self {
        let cached = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice::<RecipeFile>(&b).ok())
            // Ancien cache sans récoltables : on le relira.
            .map(|f| {
                let at = if f.gatherables.is_empty() {
                    0
                } else {
                    f.fetched_at_ms
                };
                (
                    at,
                    Arc::new(GameData {
                        recipes: f.recipes,
                        gatherables: f.gatherables,
                    }),
                )
            });
        Self {
            http: Http::new(),
            file,
            recipes: Mutex::new(cached),
            running: Mutex::new(()),
            last: Mutex::new(None),
        }
    }

    async fn recipes(&self) -> AppResult<Arc<GameData>> {
        let mut guard = self.recipes.lock().await;
        if let Some((at, r)) = guard.as_ref() {
            if now_ms() - at < RECIPES_MAX_AGE_MS {
                return Ok(r.clone());
            }
        }
        let api = CoreXivApi::new(self.http.clone());
        let fetched = match craft::fetch_recipes(&api).await {
            Ok(recipes) => craft::fetch_gatherables(&api).await.map(|g| (recipes, g)),
            Err(e) => Err(e),
        };
        match fetched {
            Ok((recipes, gatherables)) => {
                let now = now_ms();
                let file = RecipeFile {
                    fetched_at_ms: now,
                    recipes,
                    gatherables,
                };
                if let Ok(bytes) = serde_json::to_vec(&file) {
                    if let Err(e) = std::fs::write(&self.file, bytes) {
                        tracing::warn!(error = %e, "recettes : écriture du cache impossible");
                    }
                }
                let r = Arc::new(GameData {
                    recipes: file.recipes,
                    gatherables: file.gatherables,
                });
                *guard = Some((now, r.clone()));
                Ok(r)
            }
            // Hors ligne : les recettes en cache, même anciennes, suffisent.
            Err(e) => match guard.as_ref() {
                Some((_, r)) => Ok(r.clone()),
                None => Err(AppError::Other(format!("recettes indisponibles : {e}"))),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CraftRow {
    pub recipe_id: u32,
    pub item: ItemInfo,
    pub job: u8,
    pub level: u32,
    /// Unités obtenues par fabrication.
    pub amount: u32,
    pub specialist: bool,
    pub master_book: bool,
    pub expert: bool,
    /// Prix de vente estimé (NQ) sur le serveur principal.
    pub sell_price: f64,
    pub units_per_day: f64,
    /// Coût de revient d'une unité.
    pub unit_cost: f64,
    /// Bénéfice net d'une unité (taxe de vente déduite).
    pub unit_profit: f64,
    pub margin: f64,
    pub profit_per_day: f64,
    /// Fabrications conseillées pour l'horizon de vente.
    pub suggested_crafts: u32,
    /// Arbre d'une fabrication.
    pub tree: Node,
    /// Coût recalculé sur les vraies annonces pour les fabrications conseillées.
    pub real_prices: bool,
    /// Pas assez d'annonces pour tout acheter.
    pub insufficient: bool,
    /// Achats à l'HV pour les fabrications conseillées (prix réels), par serveur.
    pub purchases: Vec<RowPurchase>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowPurchase {
    pub item: u32,
    pub world_id: i64,
    pub world: String,
    pub dc: String,
    pub units: u32,
    /// Total payé, taxe d'achat comprise.
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CraftResults {
    pub computed_at_ms: i64,
    pub tax_rate: f64,
    pub recipes_considered: usize,
    /// Prix des marchands PNJ pris en compte (données des outils de dépense prêtes).
    pub npc_prices: bool,
    pub rows: Vec<CraftRow>,
    /// Noms et icônes des composants des arbres.
    pub items: HashMap<u32, ItemInfo>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CraftOptions {
    pub allow_expert: bool,
    pub allow_master: bool,
    pub include_specialist: bool,
    /// Composants récoltables à mon niveau comptés gratuits.
    #[serde(default)]
    pub gather_myself: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    done: usize,
    total: usize,
}

/// Prix de vente estimé : moyenne des ventes, plafonnée par l'annonce la moins
/// chère du serveur (on devra s'aligner dessus).
fn sell_estimate(q: &AggQuality) -> (f64, f64) {
    let (avg, units) = home_estimate(q);
    let min = q.min_listing.world.as_ref().map(|p| p.price);
    let sell = match min {
        Some(m) if m > 0.0 && avg > 0.0 => avg.min(m),
        Some(m) if avg <= 0.0 => m,
        _ => avg,
    };
    (sell, units)
}

async fn analyze(app: &AppHandle, opts: CraftOptions) -> AppResult<CraftResults> {
    let state = app.state::<AppState>();
    let cs = app.state::<CraftState>();
    let settings: Settings = settings::load(&state.db).await?;
    if settings.crafter_levels.iter().all(|&l| l == 0) {
        return Err(AppError::Invalid(
            "renseigne les niveaux de tes métiers d'artisan dans les paramètres".into(),
        ));
    }
    let worlds = worlds::load(&state.db).await?;
    let scope = scope::build(&settings, &worlds)?;
    let data = cs.recipes().await?;
    let recipes = &data.recipes;
    let levels = settings.crafter_levels.clone();

    let no_prices = HashMap::new();
    let probe = Costing::new(
        recipes,
        &no_prices,
        &levels,
        opts.allow_expert,
        opts.allow_master,
    );
    let craftable: Vec<&Recipe> = recipes
        .iter()
        .filter(|r| probe.can_craft(r) && (opts.include_specialist || !r.specialist))
        .collect();
    drop(probe);

    let marketable: HashSet<u32> = state.universalis.marketable().await?.into_iter().collect();
    let mut ids: HashSet<u32> = HashSet::new();
    for r in recipes.iter() {
        ids.extend(r.ingredients.iter().map(|&(i, _)| i));
    }
    ids.extend(craftable.iter().map(|r| r.result));
    let mut ids: Vec<u32> = ids.into_iter().filter(|i| marketable.contains(i)).collect();
    ids.sort_unstable();

    // Données agrégées de chaque DC du périmètre.
    let chunks: Vec<Vec<u32>> = ids
        .chunks(MAX_IDS_PER_REQUEST)
        .map(<[u32]>::to_vec)
        .collect();
    let jobs: Vec<(usize, usize)> = (0..chunks.len())
        .flat_map(|c| (0..scope.dcs.len()).map(move |d| (c, d)))
        .collect();
    let total = jobs.len();
    let api = &state.universalis;
    let mut stream = futures::stream::iter(jobs)
        .map(|(c, d)| {
            let world = scope.dcs[d].query_world.name.clone();
            let chunk = &chunks[c];
            async move { (d, api.aggregated(&world, chunk).await) }
        })
        .buffer_unordered(MAX_IN_FLIGHT);
    let mut home: HashMap<u32, AggItem> = HashMap::new();
    let mut sources: HashMap<u32, Vec<(usize, AggItem)>> = HashMap::new();
    let mut done = 0;
    while let Some((d, res)) = stream.next().await {
        done += 1;
        if done % 5 == 0 || done == total {
            let _ = app.emit(EVENT_CRAFT_PROGRESS, Progress { done, total });
        }
        match res {
            Ok(resp) => {
                for item in resp.results {
                    if scope.dcs[d].is_home_dc {
                        home.insert(item.item_id, item.clone());
                    }
                    sources.entry(item.item_id).or_default().push((d, item));
                }
            }
            Err(e) => tracing::warn!(error = %e, "artisanat : bloc de prix échoué"),
        }
    }
    drop(stream);

    // Prix d'achat le plus bas (NQ ou HQ) parmi les serveurs à jour.
    let now = now_ms();
    let max_age = i64::from(settings.max_data_age_hours) * 3600 * 1000;
    let mut prices: HashMap<u32, BuyPrice> = HashMap::new();
    for (&item, srcs) in &sources {
        for (_, agg) in srcs {
            for q in [&agg.nq, &agg.hq] {
                let Some(min) = q.min_listing.dc.as_ref() else {
                    continue;
                };
                let Some(world_id) = min.world_id else {
                    continue;
                };
                if !upload_of(agg, world_id).is_some_and(|u| now - u <= max_age) {
                    continue;
                }
                let Some(world) = scope.world(world_id) else {
                    continue;
                };
                let unit = min.price * (1.0 + BUYER_TAX);
                if prices.get(&item).is_none_or(|p| unit < p.unit) {
                    prices.insert(
                        item,
                        BuyPrice::market(unit, &world.name, world_id, &world.dc),
                    );
                }
            }
        }
    }

    let tax = seller_tax(&state.universalis, &scope, &settings).await;
    // Marchands PNJ : prix fixe, sans taxe d'achat.
    let dataset = app.state::<SpenderState>().dataset().await;
    if let Some(ds) = &dataset {
        for o in &ds.gil_offers {
            let unit = f64::from(o.price);
            if unit > 0.0 && prices.get(&o.item).is_none_or(|p| unit < p.unit) {
                let vendor = o
                    .vendors
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Marchand".into());
                prices.insert(
                    o.item,
                    BuyPrice {
                        unit,
                        origin: Origin::Npc { vendor },
                    },
                );
            }
        }
    }
    // Récolte : gratuite (hors temps) si le métier est assez haut.
    if opts.gather_myself {
        for g in &data.gatherables {
            let level = settings
                .gatherer_levels
                .get(g.job as usize)
                .copied()
                .unwrap_or(0);
            if level >= g.level {
                prices.insert(
                    g.item,
                    BuyPrice {
                        unit: 0.0,
                        origin: Origin::Gather {
                            job: g.job,
                            level: g.level,
                        },
                    },
                );
            }
        }
    }

    let mut costing = Costing::new(
        recipes,
        &prices,
        &levels,
        opts.allow_expert,
        opts.allow_master,
    );
    // Meilleure recette par objet fabriqué.
    let mut best: HashMap<u32, (CraftRowDraft, &Recipe)> = HashMap::new();
    for r in &craftable {
        let Some(h) = home.get(&r.result) else {
            continue;
        };
        if !upload_of(h, scope.home.id).is_some_and(|u| now - u <= max_age) {
            continue;
        }
        let (sell, units) = sell_estimate(&h.nq);
        if sell <= 0.0 || units < settings.min_units_per_day {
            continue;
        }
        let Some(cost) = costing.craft_cost(r, &mut HashSet::new()) else {
            continue;
        };
        if cost <= 0.0 {
            continue;
        }
        let profit = sell * (1.0 - tax) - cost;
        let margin = profit / cost;
        if profit <= 0.0 || margin < settings.min_margin {
            continue;
        }
        let draft = CraftRowDraft {
            sell,
            units,
            cost,
            profit,
            margin,
        };
        if best
            .get(&r.result)
            .is_none_or(|(d, _)| draft.profit > d.profit)
        {
            best.insert(r.result, (draft, r));
        }
    }
    let mut ranked: Vec<(CraftRowDraft, &Recipe)> = best.into_values().collect();
    ranked.sort_by(|a, b| (b.0.profit * b.0.units).total_cmp(&(a.0.profit * a.0.units)));
    ranked.truncate(MAX_ROWS);

    let trees: Vec<Node> = ranked
        .iter()
        .map(|(_, r)| costing.tree(r.result, f64::from(r.amount), Some(r)))
        .collect();
    let mut item_ids: HashSet<u32> = HashSet::new();
    fn collect(n: &Node, out: &mut HashSet<u32>) {
        out.insert(n.item);
        n.children.iter().for_each(|c| collect(c, out));
    }
    trees.iter().for_each(|t| collect(t, &mut item_ids));
    let item_ids: Vec<u32> = item_ids.into_iter().collect();
    let infos = item_infos(&state.xivapi, &item_ids, settings.language).await;
    let horizon = f64::from(settings.sell_horizon_days);

    let mut rows: Vec<CraftRow> = ranked
        .into_iter()
        .zip(trees)
        .filter_map(|((d, r), tree)| {
            Some(CraftRow {
                recipe_id: r.id,
                item: infos.get(&r.result)?.clone(),
                job: r.job,
                level: r.level,
                amount: r.amount,
                specialist: r.specialist,
                master_book: r.master_book,
                expert: r.expert,
                sell_price: d.sell,
                units_per_day: d.units,
                unit_cost: d.cost,
                unit_profit: d.profit,
                margin: d.margin,
                profit_per_day: d.profit * d.units,
                suggested_crafts: ((d.units * horizon) / f64::from(r.amount)).ceil().max(1.0)
                    as u32,
                tree,
                real_prices: false,
                insufficient: false,
                purchases: Vec::new(),
            })
        })
        .collect();

    // Les premiers résultats sont recalculés sur les vraies annonces, pour les
    // quantités des fabrications conseillées.
    let top = rows.len().min(REAL_PRICED_ROWS);
    if let Err(e) = real_prices(state, &scope, &mut rows[..top], tax).await {
        tracing::warn!(error = %e, "artisanat : prix réels indisponibles");
    }
    rows.retain(|r| !r.real_prices || (r.unit_profit > 0.0 && r.margin >= settings.min_margin));
    rows.sort_by(|a, b| b.profit_per_day.total_cmp(&a.profit_per_day));

    Ok(CraftResults {
        computed_at_ms: now_ms(),
        tax_rate: tax,
        recipes_considered: craftable.len(),
        npc_prices: dataset.is_some(),
        rows,
        items: infos,
    })
}

/// Objet → (quantité à acheter, coût unitaire estimé).
type MarketNeeds = HashMap<u32, (f64, f64)>;

/// Feuilles d'un arbre multipliées par `mult` : quantités à acheter à l'HV
/// par objet, et coût des autres composants (PNJ, récolte).
fn leaves(n: &Node, mult: f64, market: &mut MarketNeeds, other: &mut f64) {
    match &n.source {
        Source::Craft { .. } => n
            .children
            .iter()
            .for_each(|c| leaves(c, mult, market, other)),
        Source::Buy { .. } => {
            let e = market.entry(n.item).or_insert((0.0, n.unit_cost));
            e.0 += n.quantity * mult;
        }
        Source::Npc { .. } | Source::Gather { .. } => *other += n.unit_cost * n.quantity * mult,
        Source::Missing => {}
    }
}

async fn real_prices(
    state: &AppState,
    scope: &scope::Scope,
    rows: &mut [CraftRow],
    tax: f64,
) -> AppResult<()> {
    let mut needs: Vec<(MarketNeeds, f64)> = Vec::with_capacity(rows.len());
    let mut ids: HashSet<u32> = HashSet::new();
    for r in rows.iter() {
        let mut market = HashMap::new();
        let mut other = 0.0;
        leaves(
            &r.tree,
            f64::from(r.suggested_crafts),
            &mut market,
            &mut other,
        );
        ids.extend(market.keys().copied());
        needs.push((market, other));
    }
    let mut ids: Vec<u32> = ids.into_iter().collect();
    ids.sort_unstable();
    let mut offers: HashMap<u32, Vec<craft::Offer>> = HashMap::new();
    for dc in &scope.dcs {
        for chunk in ids.chunks(MAX_IDS_PER_REQUEST) {
            let markets = state
                .universalis
                .market(&dc.name, chunk, Some(100), CachePolicy::MaxAge(TTL_MARKET))
                .await?;
            for (item, m) in markets {
                let list = offers.entry(item).or_default();
                for l in &m.listings {
                    if let Some(w) = l.world_id.filter(|&w| scope.world(w).is_some()) {
                        list.push((l.price_per_unit as f64, l.quantity, w));
                    }
                }
            }
        }
    }
    for (r, (market, other)) in rows.iter_mut().zip(needs) {
        let mut total = other;
        let mut purchases = Vec::new();
        let mut insufficient = false;
        for (item, (need, est_unit)) in market {
            let list = offers.get(&item).map(Vec::as_slice).unwrap_or_default();
            match craft::fill(list, need) {
                Some((paid, by_world)) => {
                    total += paid * (1.0 + BUYER_TAX);
                    for (world_id, units, cost) in by_world {
                        let Some(w) = scope.world(world_id) else {
                            continue;
                        };
                        purchases.push(RowPurchase {
                            item,
                            world_id,
                            world: w.name.clone(),
                            dc: w.dc.clone(),
                            units,
                            cost: cost * (1.0 + BUYER_TAX),
                        });
                    }
                }
                None => {
                    insufficient = true;
                    total += need * est_unit;
                }
            }
        }
        let units = f64::from(r.suggested_crafts * r.amount);
        r.unit_cost = total / units;
        r.unit_profit = r.sell_price * (1.0 - tax) - r.unit_cost;
        r.margin = if r.unit_cost > 0.0 {
            r.unit_profit / r.unit_cost
        } else {
            f64::INFINITY
        };
        r.profit_per_day = r.unit_profit * r.units_per_day;
        r.real_prices = true;
        r.insufficient = insufficient;
        r.purchases = purchases;
    }
    Ok(())
}

struct CraftRowDraft {
    sell: f64,
    units: f64,
    cost: f64,
    profit: f64,
    margin: f64,
}

pub async fn craft_analyze(
    app: AppHandle,
    cs: State<'_, CraftState>,
    options: CraftOptions,
) -> AppResult<CraftResults> {
    let Ok(_running) = cs.running.try_lock() else {
        return Err(AppError::Invalid("analyse déjà en cours".into()));
    };
    let res = analyze(&app, options).await?;
    *cs.last.lock().await = Some(res.clone());
    Ok(res)
}

pub async fn craft_last(cs: State<'_, CraftState>) -> AppResult<Option<CraftResults>> {
    Ok(cs.last.lock().await.clone())
}

pub async fn craft_lodestone(cs: State<'_, CraftState>, input: String) -> AppResult<LodestoneJobs> {
    craft::fetch_lodestone(&cs.http, &input)
        .await
        .map_err(|e| AppError::Invalid(e.to_string()))
}
