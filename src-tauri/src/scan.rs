//! F4 scan: aggregated prefilter over every marketable item, then a full
//! drill-down + F5 math on the top candidates. Runs as a cancellable
//! background job and reports progress through Tauri events.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use futures::stream::{self, StreamExt};
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;

use crate::analysis::{self, ItemContext};
use crate::error::{AppError, AppResult};
use crate::net::{now_ms, CachePolicy, MAX_IN_FLIGHT};
use crate::profit::{self, Quality, PARAMS};
use crate::scope::{self, Scope};
use crate::settings::{self, Settings};
use crate::universalis::{
    AggItem, AggQuality, HistoryItem, MarketItem, Universalis, MAX_IDS_PER_REQUEST,
};
use crate::{worlds, AppState};

pub const EVENT_SCAN_PROGRESS: &str = "scan-progress";
const HOME_HISTORY_WINDOW: Duration = Duration::from_secs(7 * 24 * 3600);
/// Listings fetched per item on drill-down (cheapest first).
const DRILL_LISTINGS: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Preparing,
    Prefilter,
    DrillDown,
    Saving,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub scan_id: String,
    pub phase: Phase,
    pub done: u32,
    pub total: u32,
    pub eta_secs: Option<u32>,
    pub items_failed: u32,
    pub message: Option<String>,
}

struct Running {
    cancel: CancellationToken,
    progress: ScanProgress,
}

#[derive(Default)]
pub struct ScanManager {
    running: Mutex<Option<Running>>,
}

impl ScanManager {
    pub fn current(&self) -> Option<ScanProgress> {
        self.running
            .lock()
            .expect("scan lock")
            .as_ref()
            .map(|r| r.progress.clone())
    }

    pub fn is_running(&self) -> bool {
        self.running.lock().expect("scan lock").is_some()
    }

    pub fn cancel(&self) -> bool {
        match self.running.lock().expect("scan lock").as_ref() {
            Some(r) => {
                r.cancel.cancel();
                true
            }
            None => false,
        }
    }
}

/// Reports progress to the UI and keeps the latest value for late joiners.
type Sink = Box<dyn Fn(&ScanProgress) + Send + Sync>;

/// Computes progress/ETA and hands each update to `sink`.
struct Reporter {
    sink: Sink,
    scan_id: String,
    started: Instant,
    total: u32,
    done: u32,
    items_failed: u32,
}

impl Reporter {
    fn emit(&self, phase: Phase, message: Option<String>) {
        let eta_secs = (self.done > 0 && self.done < self.total).then(|| {
            let per = self.started.elapsed().as_secs_f64() / f64::from(self.done);
            (per * f64::from(self.total - self.done)).round() as u32
        });
        let progress = ScanProgress {
            scan_id: self.scan_id.clone(),
            phase,
            done: self.done,
            total: self.total,
            eta_secs,
            items_failed: self.items_failed,
            message,
        };
        (self.sink)(&progress);
    }
}

/// Publishes progress to the UI and keeps the latest value for late joiners.
fn app_sink(app: AppHandle) -> Sink {
    Box::new(move |progress| {
        let state = app.state::<AppState>();
        if let Some(r) = state.scans.running.lock().expect("scan lock").as_mut() {
            r.progress = progress.clone();
        }
        let _ = app.emit(EVENT_SCAN_PROGRESS, progress.clone());
    })
}

/// What a run covers.
#[derive(Debug, Clone)]
pub enum Mode {
    /// Every marketable item, as a new scan.
    Full,
    /// Only these items, merged into an existing scan.
    Retry(Vec<u32>),
}

/// Starts a full scan in the background and returns its id.
pub fn start(app: &AppHandle) -> AppResult<String> {
    let scan_id = format!("scan-{}", now_ms());
    launch(app, scan_id, Mode::Full)
}

/// Re-runs only the items that failed in `scan_id`, merging into it.
pub async fn retry_failed(app: &AppHandle, scan_id: &str) -> AppResult<()> {
    let state = app.state::<AppState>();
    let failed: Option<String> = sqlx::query_scalar(
        "SELECT failed_items FROM scan_runs WHERE scan_id = ? AND status = 'done'",
    )
    .bind(scan_id)
    .fetch_optional(&state.db)
    .await?
    .flatten();
    let ids: Vec<u32> = failed
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?
        .unwrap_or_default();
    if ids.is_empty() {
        return Err(AppError::Invalid(
            "this scan has no failed items to retry".into(),
        ));
    }
    launch(app, scan_id.to_owned(), Mode::Retry(ids))?;
    Ok(())
}

fn launch(app: &AppHandle, scan_id: String, mode: Mode) -> AppResult<String> {
    let state = app.state::<AppState>();
    let cancel = CancellationToken::new();
    {
        let mut slot = state.scans.running.lock().expect("scan lock");
        if slot.is_some() {
            return Err(AppError::Invalid("a scan is already running".into()));
        }
        *slot = Some(Running {
            cancel: cancel.clone(),
            progress: ScanProgress {
                scan_id: scan_id.clone(),
                phase: Phase::Preparing,
                done: 0,
                total: 0,
                eta_secs: None,
                items_failed: 0,
                message: None,
            },
        });
    }
    let app = app.clone();
    let id = scan_id.clone();
    tauri::async_runtime::spawn(async move {
        let mut reporter = Reporter {
            sink: app_sink(app.clone()),
            scan_id: id.clone(),
            started: Instant::now(),
            total: 0,
            done: 0,
            items_failed: 0,
        };
        let state = app.state::<AppState>();
        let outcome = run(&state, &id, &mode, &cancel, &mut reporter).await;
        let (status, phase, error) = match &outcome {
            Ok(()) => ("done", Phase::Done, None),
            Err(_) if cancel.is_cancelled() => ("cancelled", Phase::Cancelled, None),
            Err(e) => ("failed", Phase::Failed, Some(e.to_string())),
        };
        if let Some(e) = &error {
            tracing::error!(scan_id = %id, error = %e, "scan failed");
        }
        if let Err(e) = finish_run(
            &state.db,
            &id,
            status,
            reporter.items_failed,
            error.as_deref(),
        )
        .await
        {
            tracing::error!(error = %e, "cannot record scan end");
        }
        reporter.emit(phase, error);
        *state.scans.running.lock().expect("scan lock") = None;
    });
    Ok(scan_id)
}

async fn finish_run(
    db: &SqlitePool,
    id: &str,
    status: &str,
    failed: u32,
    error: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE scan_runs SET status = ?, completed_at = ?, items_failed = ?, error = ?
         WHERE scan_id = ?",
    )
    .bind(status)
    .bind(now_ms())
    .bind(failed)
    .bind(error)
    .bind(id)
    .execute(db)
    .await?;
    Ok(())
}

/// Await `fut` unless the scan is cancelled first.
async fn or_cancel<T>(
    cancel: &CancellationToken,
    fut: impl std::future::Future<Output = T>,
) -> AppResult<T> {
    tokio::select! {
        _ = cancel.cancelled() => Err(AppError::Other("scan cancelled".into())),
        v = fut => Ok(v),
    }
}

/// Home-world anchor from the aggregated response (world level).
pub(crate) fn home_estimate(q: &AggQuality) -> (f64, f64) {
    let sell = q
        .average_sale_price
        .world
        .as_ref()
        .or(q.recent_purchase.world.as_ref())
        .map_or(0.0, |p| p.price);
    let units_per_day = q
        .daily_sale_velocity
        .world
        .as_ref()
        .map_or(0.0, |v| v.quantity);
    (sell, units_per_day)
}

fn quality_of(item: &AggItem, q: Quality) -> &AggQuality {
    match q {
        Quality::NQ => &item.nq,
        Quality::HQ => &item.hq,
    }
}

pub(crate) fn upload_of(item: &AggItem, world_id: i64) -> Option<i64> {
    item.world_upload_times
        .iter()
        .find(|u| u.world_id == world_id)
        .map(|u| u.timestamp)
}

#[derive(Debug, Clone)]
struct Candidate {
    item_id: u32,
    quality: Quality,
    sold_as: Quality,
    dc_index: usize,
    world_id: i64,
    min_price: f64,
    est_sell: f64,
    est_profit: f64,
    est_score: f64,
    home_units_per_day: f64,
    upload_ms: Option<i64>,
}

/// F4 steps 4-5 for one item.
fn prefilter_item(
    item_id: u32,
    home: &AggItem,
    sources: &[(usize, &AggItem)],
    scope: &Scope,
    settings: &Settings,
    seller_tax: f64,
    now: i64,
) -> Vec<Candidate> {
    let max_age = i64::from(settings.max_data_age_hours) * 3600 * 1000;
    let fresh = |upload: Option<i64>| upload.is_some_and(|u| now - u <= max_age);
    if !fresh(upload_of(home, scope.home.id)) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for &(dc_index, agg) in sources {
        for source_q in [Quality::NQ, Quality::HQ] {
            let Some(min) = quality_of(agg, source_q).min_listing.dc.as_ref() else {
                continue;
            };
            let Some(world_id) = min.world_id.filter(|&w| w != scope.home.id) else {
                continue;
            };
            let upload = upload_of(agg, world_id);
            if !fresh(upload) {
                continue;
            }
            let targets: &[Quality] = match source_q {
                Quality::NQ => &[Quality::NQ],
                Quality::HQ => &[Quality::HQ, Quality::NQ],
            };
            let best = targets
                .iter()
                .filter_map(|&sold_as| {
                    let (est_sell, units) = home_estimate(quality_of(home, sold_as));
                    let (profit, score) = profit::prefilter(
                        est_sell,
                        units,
                        min.price,
                        seller_tax,
                        settings.min_margin,
                        settings.min_units_per_day,
                        &PARAMS,
                    )?;
                    Some(Candidate {
                        item_id,
                        quality: source_q,
                        sold_as,
                        dc_index,
                        world_id,
                        min_price: min.price,
                        est_sell,
                        est_profit: profit,
                        est_score: score,
                        home_units_per_day: units,
                        upload_ms: upload,
                    })
                })
                .max_by(|a, b| a.est_score.total_cmp(&b.est_score));
            out.extend(best);
        }
    }
    out
}

async fn run(
    state: &AppState,
    scan_id: &str,
    mode: &Mode,
    cancel: &CancellationToken,
    rep: &mut Reporter,
) -> AppResult<()> {
    let db = &state.db;
    let api = &state.universalis;
    let settings = settings::load(db).await?;
    let scope = scope::build(&settings, &worlds::load(db).await?)?;
    match mode {
        Mode::Full => {
            sqlx::query(
                "INSERT INTO scan_runs (scan_id, started_at, status) VALUES (?, ?, 'running')",
            )
            .bind(scan_id)
            .bind(now_ms())
            .execute(db)
            .await?
        }
        Mode::Retry(_) => {
            sqlx::query("UPDATE scan_runs SET status = 'running', error = NULL WHERE scan_id = ?")
                .bind(scan_id)
                .execute(db)
                .await?
        }
    };
    rep.emit(Phase::Preparing, None);

    let seller_tax = seller_tax(api, &scope, &settings).await;
    let mine = crate::ventes::normalize_names(&settings.retainer_names);
    let ids = match mode {
        Mode::Full => or_cancel(cancel, api.marketable()).await??,
        Mode::Retry(ids) => ids.clone(),
    };
    let chunks: Vec<Vec<u32>> = ids
        .chunks(MAX_IDS_PER_REQUEST)
        .map(<[u32]>::to_vec)
        .collect();
    let drill_chunks = (settings.drill_down_top_n as usize).div_ceil(MAX_IDS_PER_REQUEST);
    rep.total = (chunks.len() * scope.dcs.len() + drill_chunks * (2 + scope.dcs.len())) as u32;
    tracing::info!(
        scan_id,
        items = ids.len(),
        dcs = scope.dcs.len(),
        seller_tax,
        "scan started"
    );

    // ---- Phase 1: aggregated prefilter ----
    let jobs: Vec<(usize, usize)> = (0..chunks.len())
        .flat_map(|c| (0..scope.dcs.len()).map(move |d| (c, d)))
        .collect();
    let mut stream = stream::iter(jobs)
        .map(|(c, d)| {
            let world = scope.dcs[d].query_world.name.clone();
            let chunk = &chunks[c];
            async move { (c, d, api.aggregated(&world, chunk).await) }
        })
        .buffer_unordered(MAX_IN_FLIGHT);
    let home_dc = scope.dcs.iter().position(|d| d.is_home_dc);
    let mut home: HashMap<u32, AggItem> = HashMap::new();
    let mut sources: HashMap<u32, Vec<(usize, AggItem)>> = HashMap::new();
    let mut failed: HashSet<u32> = HashSet::new();
    rep.emit(Phase::Prefilter, None);
    while let Some((c, d, res)) = or_cancel(cancel, stream.next()).await? {
        rep.done += 1;
        match res {
            Ok(resp) => {
                failed.extend(resp.failed_items.iter().copied());
                for item in resp.results {
                    if Some(d) == home_dc {
                        home.insert(item.item_id, item.clone());
                    }
                    sources.entry(item.item_id).or_default().push((d, item));
                }
            }
            Err(e) => {
                tracing::warn!(dc = %scope.dcs[d].name, error = %e, "aggregated chunk failed");
                failed.extend(chunks[c].iter().copied());
            }
        }
        rep.items_failed = failed.len() as u32;
        rep.emit(Phase::Prefilter, None);
    }
    drop(stream);

    let now = now_ms();
    let mut candidates: Vec<Candidate> = Vec::new();
    for (item_id, home_agg) in &home {
        let srcs: Vec<(usize, &AggItem)> = sources
            .get(item_id)
            .map(|v| v.iter().map(|(d, a)| (*d, a)).collect())
            .unwrap_or_default();
        candidates.extend(prefilter_item(
            *item_id, home_agg, &srcs, &scope, &settings, seller_tax, now,
        ));
    }
    save_candidates(db, scan_id, &scope, &candidates).await?;

    // Best score per item decides the drill-down set.
    let mut best: HashMap<u32, f64> = HashMap::new();
    for c in &candidates {
        let e = best.entry(c.item_id).or_insert(0.0);
        *e = e.max(c.est_score);
    }
    let mut ranked: Vec<(u32, f64)> = best.into_iter().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    let top = select_drill_down(state, cancel, &ranked, settings.drill_down_top_n as usize).await?;
    tracing::info!(
        scan_id,
        candidates = candidates.len(),
        drill = top.len(),
        "prefilter done"
    );

    // ---- Phase 2: drill-down ----
    let top_set: HashSet<u32> = top.iter().copied().collect();
    let mut per_dc: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
    for c in candidates.iter().filter(|c| top_set.contains(&c.item_id)) {
        let ids = per_dc.entry(c.dc_index).or_default();
        if !ids.contains(&c.item_id) {
            ids.push(c.item_id);
        }
    }
    enum Job {
        HomeMarket(Vec<u32>),
        HomeHistory(Vec<u32>),
        Source(usize, Vec<u32>),
    }
    enum Out {
        HomeMarket(HashMap<u32, MarketItem>),
        HomeHistory(HashMap<u32, HistoryItem>),
        Source(HashMap<u32, MarketItem>),
    }
    let mut jobs: Vec<Job> = Vec::new();
    for chunk in top.chunks(MAX_IDS_PER_REQUEST) {
        jobs.push(Job::HomeMarket(chunk.to_vec()));
        jobs.push(Job::HomeHistory(chunk.to_vec()));
    }
    for (dc, ids) in &per_dc {
        for chunk in ids.chunks(MAX_IDS_PER_REQUEST) {
            jobs.push(Job::Source(*dc, chunk.to_vec()));
        }
    }
    // Re-base the total on the real drill-down size.
    rep.total = rep.done + jobs.len() as u32 + 1;
    let home_name = scope.home.name.clone();
    let mut stream = stream::iter(jobs)
        .map(|job| {
            let home_name = home_name.clone();
            let scope = &scope;
            async move {
                match job {
                    Job::HomeMarket(ids) => {
                        let r = api
                            .market(&home_name, &ids, Some(DRILL_LISTINGS), CachePolicy::Refresh)
                            .await;
                        (ids, r.map(Out::HomeMarket))
                    }
                    Job::HomeHistory(ids) => {
                        let r = api
                            .history(&home_name, &ids, HOME_HISTORY_WINDOW, CachePolicy::Refresh)
                            .await;
                        (ids, r.map(Out::HomeHistory))
                    }
                    Job::Source(dc, ids) => {
                        let r = api
                            .market(
                                &scope.dcs[dc].name,
                                &ids,
                                Some(DRILL_LISTINGS),
                                CachePolicy::Refresh,
                            )
                            .await;
                        (ids, r.map(Out::Source))
                    }
                }
            }
        })
        .buffer_unordered(MAX_IN_FLIGHT);
    let mut home_market: HashMap<u32, MarketItem> = HashMap::new();
    let mut home_history: HashMap<u32, HistoryItem> = HashMap::new();
    let mut source_market: HashMap<u32, Vec<MarketItem>> = HashMap::new();
    rep.emit(Phase::DrillDown, None);
    while let Some((ids, res)) = or_cancel(cancel, stream.next()).await? {
        rep.done += 1;
        match res {
            Ok(Out::HomeMarket(m)) => home_market.extend(m),
            Ok(Out::HomeHistory(h)) => home_history.extend(h),
            Ok(Out::Source(m)) => {
                for (id, item) in m {
                    source_market.entry(id).or_default().push(item);
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "drill-down chunk failed");
                failed.extend(ids);
            }
        }
        rep.items_failed = failed.len() as u32;
        rep.emit(Phase::DrillDown, None);
    }
    drop(stream);

    // ---- F5 + save ----
    rep.emit(Phase::Saving, None);
    let now = now_ms();
    let mut found = 0u32;
    let mut opp_items: Vec<u32> = Vec::new();
    let mut tx = db.begin().await?;
    for &item_id in &top {
        let (Some(hm), Some(hh)) = (home_market.get(&item_id), home_history.get(&item_id)) else {
            continue;
        };
        let hm = analysis::without_mine(hm, &mine);
        let home_listings = analysis::listings_by_world(&hm, Some(scope.home.id))
            .remove(&scope.home.id)
            .unwrap_or_default();
        let home_sales = analysis::sales_of(hh);
        let mut ladders = BTreeMap::new();
        let mut uploads = HashMap::new();
        for m in source_market.get(&item_id).into_iter().flatten() {
            for (world, ladder) in analysis::listings_by_world(m, None) {
                ladders.entry(world).or_insert_with(Vec::new).extend(ladder);
            }
            uploads.extend(analysis::upload_times(m));
        }
        let ctx = ItemContext {
            home_world_id: scope.home.id,
            home_sales: &home_sales,
            home_listings: &home_listings,
            seller_tax,
            sell_horizon_days: f64::from(settings.sell_horizon_days),
            max_data_age_ms: i64::from(settings.max_data_age_hours) * 3600 * 1000,
            now_ms: now,
        };
        let analysis = analysis::analyze_item(&ctx, &ladders, &uploads, &PARAMS);
        let mut any = false;
        for s in analysis.sources.iter().filter(|s| {
            s.eval.profit_per_trip >= settings.min_profit_per_trip as f64
                && s.eval.home_units_per_day >= settings.min_units_per_day
        }) {
            let e = &s.eval;
            sqlx::query(
                "INSERT OR REPLACE INTO opportunities
                   (scan_id, item_id, quality, source_world_id, source_unit_price, units_to_buy,
                    source_cost, expected_sell_price, home_median_price, home_units_per_day,
                    days_of_supply, unit_profit, profit_per_trip, daily_profit, confidence,
                    source_upload_unix, computed_at, sold_as, is_stale, max_unit_price, lots)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(scan_id)
            .bind(item_id)
            .bind(s.quality.as_str())
            .bind(s.world_id)
            .bind(e.source_unit_price as i64)
            .bind(e.units_to_buy as i64)
            .bind(e.source_cost.round() as i64)
            .bind(e.expected_sell.round() as i64)
            .bind(e.home_median.round() as i64)
            .bind(e.home_units_per_day)
            .bind(e.days_of_supply)
            .bind(e.unit_profit.round() as i64)
            .bind(e.profit_per_trip.round() as i64)
            .bind(e.daily_profit.round() as i64)
            .bind(e.confidence.as_str())
            .bind(s.source_upload_ms.map(|ms| ms / 1000))
            .bind(now)
            .bind(e.sold_as.as_str())
            .bind(s.is_stale)
            .bind(e.max_unit_price as i64)
            .bind(serde_json::to_string(&e.lots).ok())
            .execute(&mut *tx)
            .await?;
            found += 1;
            any = true;
        }
        if any {
            opp_items.push(item_id);
        }
    }
    let mut failed_ids: Vec<u32> = failed.iter().copied().collect();
    failed_ids.sort_unstable();
    // A retry keeps the original item count; opportunities are recounted
    // because the retried items are added to the existing ones.
    sqlx::query(
        "UPDATE scan_runs SET
           items_scanned = CASE WHEN ? THEN ? ELSE items_scanned END,
           opportunities_found = (SELECT COUNT(*) FROM opportunities WHERE scan_id = ?),
           items_failed = ?, failed_items = ?
         WHERE scan_id = ?",
    )
    .bind(matches!(mode, Mode::Full))
    .bind(ids.len() as i64)
    .bind(scan_id)
    .bind(failed_ids.len() as i64)
    .bind(serde_json::to_string(&failed_ids)?)
    .bind(scan_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    for id in &opp_items {
        if let Some(h) = home_history.get(id) {
            if let Err(e) = analysis::store_sales(db, scope.home.id, h).await {
                tracing::warn!(error = %e, "cannot store sales history");
            }
        }
    }
    // Names and icons for the results table; a failure only costs labels.
    if let Err(e) = or_cancel(cancel, state.xivapi.items(&opp_items)).await? {
        tracing::warn!(error = %e, "item metadata fetch failed");
    }
    rep.done = rep.total;
    tracing::info!(scan_id, found, failed = failed.len(), "scan done");
    prune_other_scans(db, scan_id).await;
    Ok(())
}

/// Best-ranked items for the drill-down, skipping housing items (out of
/// scope). Metadata is fetched in ranking order, 100 at a time, only as far
/// as needed; it is cached permanently, so later scans hit the network less.
async fn select_drill_down(
    state: &AppState,
    cancel: &CancellationToken,
    ranked: &[(u32, f64)],
    n: usize,
) -> AppResult<Vec<u32>> {
    let mut top = Vec::with_capacity(n);
    for batch in ranked.chunks(MAX_IDS_PER_REQUEST) {
        if top.len() >= n {
            break;
        }
        let ids: Vec<u32> = batch.iter().map(|(id, _)| *id).collect();
        // Unknown metadata (XIVAPI down) must not block the scan: keep the item.
        let meta = or_cancel(cancel, state.xivapi.items(&ids))
            .await?
            .unwrap_or_default();
        for id in ids {
            if top.len() < n && !meta.get(&id).is_some_and(|m| m.is_housing) {
                top.push(id);
            }
        }
    }
    Ok(top)
}

/// Seller tax for the configured retainer city; 5% (the maximum) if unknown.
pub async fn seller_tax(api: &Universalis, scope: &Scope, settings: &Settings) -> f64 {
    let city = serde_json::to_value(settings.retainer_city)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    match api.tax_rates(&scope.home.name).await {
        Ok(rates) => rates.get(&city).map_or(0.05, |&pct| f64::from(pct) / 100.0),
        Err(e) => {
            tracing::warn!(error = %e, "tax rates unavailable, assuming 5%");
            0.05
        }
    }
}

async fn save_candidates(
    db: &SqlitePool,
    scan_id: &str,
    scope: &Scope,
    candidates: &[Candidate],
) -> AppResult<()> {
    let mut tx = db.begin().await?;
    for c in candidates {
        sqlx::query(
            "INSERT OR REPLACE INTO scan_candidates
               (scan_id, item_id, quality, sold_as, source_dc, source_world_id, source_min_price,
                est_sell_price, est_profit, est_score, home_units_per_day, source_upload_unix)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(scan_id)
        .bind(c.item_id)
        .bind(c.quality.as_str())
        .bind(c.sold_as.as_str())
        .bind(&scope.dcs[c.dc_index].name)
        .bind(c.world_id)
        .bind(c.min_price.round() as i64)
        .bind(c.est_sell.round() as i64)
        .bind(c.est_profit.round() as i64)
        .bind(c.est_score)
        .bind(c.home_units_per_day)
        .bind(c.upload_ms.map(|ms| ms / 1000))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::universalis::AggResponse;
    use crate::worlds;

    const NOW: i64 = 1_800_000_000_000;

    fn agg(json: &str) -> AggItem {
        let r: AggResponse = serde_json::from_str(&format!(r#"{{"results":[{json}]}}"#)).unwrap();
        r.results.into_iter().next().unwrap()
    }

    #[test]
    fn prefilter_uses_dc_min_and_skips_home_and_stale() {
        let settings = Settings {
            home_world_id: Some(83),
            ..Settings::default()
        };
        let scope = scope::build(&settings, &worlds::bundled()).unwrap();
        let fresh = NOW - 3_600_000;
        let old = NOW - 48 * 3_600_000;
        // Home (Louisoix, 83): sells ~1000 NQ, 3 units/day.
        let home = agg(&format!(
            r#"{{"itemId":1,"nq":{{"minListing":{{"dc":{{"price":10,"worldId":83}}}},
                "averageSalePrice":{{"world":{{"price":1000}}}},
                "dailySaleVelocity":{{"world":{{"quantity":3}}}}}},
               "worldUploadTimes":[{{"worldId":83,"timestamp":{fresh}}}]}}"#
        ));
        // Light: cheapest NQ on Alpha (402), fresh. HQ min on Odin (66) is stale.
        let light = agg(&format!(
            r#"{{"itemId":1,"nq":{{"minListing":{{"dc":{{"price":400,"worldId":402}}}}}},
               "hq":{{"minListing":{{"dc":{{"price":300,"worldId":66}}}}}},
               "worldUploadTimes":[{{"worldId":402,"timestamp":{fresh}}},{{"worldId":66,"timestamp":{old}}}]}}"#
        ));
        let light_idx = scope.dcs.iter().position(|d| d.name == "Light").unwrap();
        let home_idx = scope.dcs.iter().position(|d| d.is_home_dc).unwrap();
        let out = prefilter_item(
            1,
            &home,
            &[(home_idx, &home), (light_idx, &light)],
            &scope,
            &settings,
            0.05,
            NOW,
        );
        assert_eq!(
            out.len(),
            1,
            "home-DC min is on the home world itself; HQ source is stale"
        );
        let c = &out[0];
        assert_eq!(
            (c.world_id, c.quality, c.sold_as),
            (402, Quality::NQ, Quality::NQ)
        );
        // 1000 × 0.95 − 400 × 1.05 = 530
        assert!((c.est_profit - 530.0).abs() < 1e-9);
    }

    #[test]
    fn prefilter_skips_items_with_stale_home() {
        let settings = Settings {
            home_world_id: Some(83),
            ..Settings::default()
        };
        let scope = scope::build(&settings, &worlds::bundled()).unwrap();
        let home = agg(&format!(
            r#"{{"itemId":1,"nq":{{"averageSalePrice":{{"world":{{"price":1000}}}},
                "dailySaleVelocity":{{"world":{{"quantity":3}}}}}},
               "worldUploadTimes":[{{"worldId":83,"timestamp":{}}}]}}"#,
            NOW - 72 * 3_600_000
        ));
        assert!(prefilter_item(1, &home, &[(0, &home)], &scope, &settings, 0.05, NOW).is_empty());
    }
}

/// End-to-end check against the live APIs on a slice of the catalogue:
/// `cargo test live_pipeline -- --ignored --nocapture`
#[cfg(test)]
mod live {
    use super::*;
    use crate::{db, net::Throttled};

    #[tokio::test]
    #[ignore = "hits the network"]
    async fn live_pipeline() {
        let pool = db::open_memory().await;
        let api = Universalis::new(Throttled::new(reqwest::Client::new()), pool.clone());
        let settings = Settings {
            home_world_id: Some(83),
            ..Settings::default()
        };
        let scope = scope::build(&settings, &worlds::bundled()).unwrap();
        let tax = seller_tax(&api, &scope, &settings).await;
        let ids = api.marketable().await.unwrap();
        let sample: Vec<u32> = ids.iter().copied().skip(4000).take(1000).collect();

        let mut home: HashMap<u32, AggItem> = HashMap::new();
        let mut sources: HashMap<u32, Vec<(usize, AggItem)>> = HashMap::new();
        for chunk in sample.chunks(MAX_IDS_PER_REQUEST) {
            for (d, dc) in scope.dcs.iter().enumerate() {
                for item in api
                    .aggregated(&dc.query_world.name, chunk)
                    .await
                    .unwrap()
                    .results
                {
                    if dc.is_home_dc {
                        home.insert(item.item_id, item.clone());
                    }
                    sources.entry(item.item_id).or_default().push((d, item));
                }
            }
        }
        let now = now_ms();
        let mut cands: Vec<Candidate> = home
            .iter()
            .flat_map(|(id, h)| {
                let srcs: Vec<(usize, &AggItem)> =
                    sources[id].iter().map(|(d, a)| (*d, a)).collect();
                prefilter_item(*id, h, &srcs, &scope, &settings, tax, now)
            })
            .collect();
        cands.sort_by(|a, b| b.est_score.total_cmp(&a.est_score));
        println!(
            "tax={tax} items={} candidates={}",
            sample.len(),
            cands.len()
        );

        let mut top: Vec<u32> = Vec::new();
        for c in &cands {
            if !top.contains(&c.item_id) && top.len() < 20 {
                top.push(c.item_id);
            }
        }
        let hm = api
            .market(
                &scope.home.name,
                &top,
                Some(DRILL_LISTINGS),
                CachePolicy::Refresh,
            )
            .await
            .unwrap();
        let hh = api
            .history(
                &scope.home.name,
                &top,
                HOME_HISTORY_WINDOW,
                CachePolicy::Refresh,
            )
            .await
            .unwrap();
        let mut srcm: HashMap<u32, Vec<MarketItem>> = HashMap::new();
        for dc in &scope.dcs {
            for (id, m) in api
                .market(&dc.name, &top, Some(DRILL_LISTINGS), CachePolicy::Refresh)
                .await
                .unwrap()
            {
                srcm.entry(id).or_default().push(m);
            }
        }
        let mut found = 0;
        for id in &top {
            let (Some(m), Some(h)) = (hm.get(id), hh.get(id)) else {
                continue;
            };
            let home_listings = analysis::listings_by_world(m, Some(scope.home.id))
                .remove(&scope.home.id)
                .unwrap_or_default();
            let sales = analysis::sales_of(h);
            let mut ladders = BTreeMap::new();
            let mut uploads = HashMap::new();
            for sm in &srcm[id] {
                for (w, l) in analysis::listings_by_world(sm, None) {
                    ladders.entry(w).or_insert_with(Vec::new).extend(l);
                }
                uploads.extend(analysis::upload_times(sm));
            }
            let ctx = ItemContext {
                home_world_id: scope.home.id,
                home_sales: &sales,
                home_listings: &home_listings,
                seller_tax: tax,
                sell_horizon_days: 3.0,
                max_data_age_ms: 24 * 3_600_000,
                now_ms: now,
            };
            let a = analysis::analyze_item(&ctx, &ladders, &uploads, &PARAMS);
            if let Some(s) = a.sources.first() {
                found += 1;
                println!(
                    "item {id}: buy {} {:?} @{} on {} → sell {:.0} ({:?}), ppt {:.0}, daily {:.0}, {:?}, {:.1} u/d",
                    s.eval.units_to_buy, s.quality, s.eval.source_unit_price, s.world_id,
                    s.eval.expected_sell, s.eval.sold_as, s.eval.profit_per_trip,
                    s.eval.daily_profit, s.eval.confidence, s.eval.home_units_per_day
                );
            }
        }
        println!("drilled={} with_opportunity={found}", top.len());
        assert!(
            !cands.is_empty(),
            "expected some prefilter candidates in 1000 items"
        );
    }
}

/// Garde uniquement le scan qui vient de se terminer : les prix changent en
/// quelques heures, un ancien scan ne sert plus à décider quoi acheter. Un scan
/// en cours (relance des objets en échec) est conservé.
async fn prune_other_scans(db: &SqlitePool, keep_id: &str) {
    let result: Result<(), crate::error::AppError> = async {
        #[derive(sqlx::FromRow)]
        struct IdRow {
            scan_id: String,
        }

        let old: Vec<IdRow> = sqlx::query_as(
            "SELECT scan_id FROM scan_runs WHERE scan_id <> ? AND status <> 'running'",
        )
        .bind(keep_id)
        .fetch_all(db)
        .await?;

        for row in old {
            let id = &row.scan_id;
            sqlx::query("DELETE FROM opportunities WHERE scan_id = ?")
                .bind(id)
                .execute(db)
                .await?;
            sqlx::query("DELETE FROM scan_candidates WHERE scan_id = ?")
                .bind(id)
                .execute(db)
                .await?;
            sqlx::query("DELETE FROM scan_runs WHERE scan_id = ?")
                .bind(id)
                .execute(db)
                .await?;
            tracing::debug!(scan_id = id, "ancien scan supprimé");
        }
        Ok(())
    }
    .await;

    if let Err(e) = result {
        tracing::warn!(error = %e, "suppression des anciens scans impossible (sans gravité)");
    }
}

/// Full `run()` against the live APIs with an in-memory database (~2 min):
/// `cargo test live_full_scan -- --ignored --nocapture`
#[cfg(test)]
mod live_full {
    use super::*;
    use crate::{db, net::Throttled, results, xivapi::XivApi};

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "hits the network"]
    async fn live_retry_merges_into_scan() {
        let pool = db::open_memory().await;
        worlds::ensure_seeded(&pool).await.unwrap();
        let s = Settings {
            home_world_id: Some(83),
            ..Settings::default()
        };
        settings::save(&pool, &s, &worlds::bundled()).await.unwrap();
        let http = reqwest::Client::new();
        let state = AppState {
            universalis: Universalis::new(Throttled::new(http.clone()), pool.clone()),
            xivapi: XivApi::new(
                http,
                pool.clone(),
                std::env::temp_dir().join("ffxiv-trader-test-icons"),
            ),
            scans: ScanManager::default(),
            watch: crate::watch::WatchManager::new().0,
            db: pool.clone(),
            data_dir: std::env::temp_dir(),
            log_dir: std::env::temp_dir(),
            _log_guard: None,
        };
        // A finished scan where 300 items "failed".
        let failed: Vec<u32> = state
            .universalis
            .marketable()
            .await
            .unwrap()
            .into_iter()
            .skip(4000)
            .take(300)
            .collect();
        sqlx::query(
            "INSERT INTO scan_runs (scan_id, started_at, completed_at, items_scanned, items_failed,
               opportunities_found, status, failed_items) VALUES ('s', 1, 2, 16845, 300, 0, 'done', ?)",
        )
        .bind(serde_json::to_string(&failed).unwrap())
        .execute(&pool)
        .await
        .unwrap();
        let mut rep = Reporter {
            sink: Box::new(|_| {}),
            scan_id: "s".into(),
            started: Instant::now(),
            total: 0,
            done: 0,
            items_failed: 0,
        };
        run(
            &state,
            "s",
            &Mode::Retry(failed),
            &CancellationToken::new(),
            &mut rep,
        )
        .await
        .unwrap();
        let (scanned, found, failed_n, status): (i64, i64, i64, String) = sqlx::query_as(
            "SELECT items_scanned, opportunities_found, items_failed, status FROM scan_runs WHERE scan_id = 's'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let opps: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM opportunities WHERE scan_id = 's'")
                .fetch_one(&pool)
                .await
                .unwrap();
        println!("scanned={scanned} found={found} opps={opps} failed={failed_n} status={status}");
        assert_eq!(scanned, 16845, "a retry keeps the original item count");
        assert_eq!(found, opps);
        assert_eq!(failed_n, 0);
        assert!(opps > 0, "300 items should yield some opportunities");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "hits the network for minutes"]
    async fn live_full_scan() {
        let pool = db::open_memory().await;
        worlds::ensure_seeded(&pool).await.unwrap();
        let s = Settings {
            home_world_id: Some(83),
            ..Settings::default()
        };
        settings::save(&pool, &s, &worlds::bundled()).await.unwrap();
        let http = reqwest::Client::new();
        let state = AppState {
            universalis: Universalis::new(Throttled::new(http.clone()), pool.clone()),
            xivapi: XivApi::new(
                http,
                pool.clone(),
                std::env::temp_dir().join("ffxiv-trader-test-icons"),
            ),
            scans: ScanManager::default(),
            watch: crate::watch::WatchManager::new().0,
            db: pool.clone(),
            data_dir: std::env::temp_dir(),
            log_dir: std::env::temp_dir(),
            _log_guard: None,
        };
        let last = std::sync::Arc::new(Mutex::new(None::<ScanProgress>));
        let seen = last.clone();
        let mut rep = Reporter {
            sink: Box::new(move |p| *seen.lock().unwrap() = Some(p.clone())),
            scan_id: "live".into(),
            started: Instant::now(),
            total: 0,
            done: 0,
            items_failed: 0,
        };
        let t0 = Instant::now();
        run(
            &state,
            "live",
            &Mode::Full,
            &CancellationToken::new(),
            &mut rep,
        )
        .await
        .unwrap();
        finish_run(&pool, "live", "done", rep.items_failed, None)
            .await
            .unwrap();
        println!(
            "scan took {:?}, last progress {:?}",
            t0.elapsed(),
            last.lock().unwrap()
        );

        let r = results::scan_results(&pool, &state.xivapi, &s, &worlds::bundled(), None)
            .await
            .unwrap()
            .expect("a done scan");
        println!(
            "run: {:?}\nopportunities: {}  trips: {}",
            r.run,
            r.opportunities.len(),
            r.trips.len()
        );
        for o in r.opportunities.iter().take(10) {
            println!(
                "{:<40} {} {:>10} x{:<4} on {:<10} sell {:>8} ppt {:>9} day {:>8} {}",
                o.item.name,
                o.quality,
                o.source_unit_price,
                o.units_to_buy,
                o.source_world,
                o.expected_sell_price,
                o.profit_per_trip,
                o.daily_profit,
                o.confidence
            );
        }
        for t in r.trips.iter().take(3) {
            println!(
                "trip {} ({}): {} gil/trip, {} gil/h, {} slots",
                t.world, t.dc, t.profit_per_trip, t.profit_per_hour, t.slots_used
            );
        }
        assert!(!r.opportunities.is_empty());
    }
}
