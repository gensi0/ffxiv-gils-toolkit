//! F6 Watchlist: starred items kept live through the Universalis WebSocket.
//!
//! Each watched item gets a REST snapshot (listings for every DC in scope +
//! 7 days of home sales), then `listings/add`, `listings/remove` and
//! `sales/add` events are applied on top and F5 is recomputed. Messages are
//! BSON both ways. Subscriptions are per item (`listings{item=X}`, which
//! covers add + remove, and `sales/add{item=X}`); worlds outside the scope
//! are dropped client-side. Per-item filters keep traffic low: the server
//! queue holds only 30 messages per client and drops the oldest.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::analysis::{self, ItemContext, SourceOpportunity};
use crate::error::{AppError, AppResult};
use crate::net::{now_ms, CachePolicy};
use crate::profit::{ListingRec, Quality, SaleRec, PARAMS};
use crate::results::item_infos;
use crate::scope::{self, Scope};
use crate::settings::{self, Settings};
use crate::xivapi::ItemInfo;
use crate::{scan, worlds, AppState};

pub const EVENT_WATCH_UPDATE: &str = "watch-update";
pub const EVENT_WS_STATUS: &str = "ws-status";
const WS_URL: &str = "wss://universalis.app/api/ws";
const HOME_HISTORY_WINDOW: Duration = Duration::from_secs(7 * 24 * 3600);
const SNAPSHOT_REFRESH: Duration = Duration::from_secs(10 * 60);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// A proxy in front of Universalis drops connections idle for 60s (the
/// server's own keep-alive is 2 min), so the client pings more often.
const PING_INTERVAL: Duration = Duration::from_secs(25);
const NOTIFY_COOLDOWN_MS: i64 = 30 * 60 * 1000;
/// Relative move of the best profit per trip that counts as a change.
const CHANGE_RATIO: f64 = 0.10;

// ---------- wire format ----------

#[derive(Serialize)]
struct ClientMsg<'a> {
    event: &'a str,
    channel: String,
}

#[derive(Debug, Deserialize)]
struct ServerMsg {
    event: String,
    item: Option<i64>,
    world: Option<i64>,
    #[serde(default)]
    listings: Vec<WsListing>,
    #[serde(default)]
    sales: Vec<WsSale>,
    reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WsListing {
    #[serde(rename = "listingID")]
    listing_id: Option<String>,
    price_per_unit: i64,
    quantity: i64,
    hq: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WsSale {
    hq: bool,
    price_per_unit: i64,
    quantity: i64,
    timestamp: i64,
}

fn channels(item: u32) -> [String; 2] {
    [
        format!("listings{{item={item}}}"),
        format!("sales/add{{item={item}}}"),
    ]
}

fn encode(event: &str, channel: String) -> AppResult<Message> {
    let bytes = bson::serialize_to_vec(&ClientMsg { event, channel })
        .map_err(|e| AppError::Other(format!("BSON encode: {e}")))?;
    Ok(Message::Binary(bytes.into()))
}

fn decode(bytes: &[u8]) -> AppResult<ServerMsg> {
    bson::deserialize_from_slice(bytes).map_err(|e| AppError::Other(format!("BSON decode: {e}")))
}

// ---------- live state ----------

#[derive(Debug, Clone)]
struct LiveListing {
    id: Option<String>,
    rec: ListingRec,
}

fn listing_rec(price: i64, qty: i64, hq: bool) -> ListingRec {
    ListingRec {
        price_per_unit: price.max(0) as u64,
        quantity: qty.clamp(0, i64::from(u32::MAX)) as u32,
        hq,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Change {
    /// An opportunity appeared where there was none.
    New,
    /// Best profit per trip rose by ≥ 10%.
    Up,
    /// Best profit per trip fell by ≥ 10% or the opportunity vanished.
    Down,
}

#[derive(Debug, Default)]
struct ItemState {
    info: Option<ItemInfo>,
    listings: BTreeMap<i64, Vec<LiveListing>>,
    uploads: HashMap<i64, i64>,
    home_sales: Vec<SaleRec>,
    loaded: bool,
    error: Option<String>,
    best: Option<SourceOpportunity>,
    change: Option<Change>,
    changed_at: Option<i64>,
    last_event_ms: Option<i64>,
    last_notified_ms: Option<i64>,
}

impl ItemState {
    fn apply_add(&mut self, world: i64, listings: &[WsListing]) {
        let ladder = self.listings.entry(world).or_default();
        for l in listings {
            let live = LiveListing {
                id: l.listing_id.clone(),
                rec: listing_rec(l.price_per_unit, l.quantity, l.hq),
            };
            match ladder
                .iter_mut()
                .find(|x| x.id.is_some() && x.id == live.id)
            {
                Some(existing) => *existing = live,
                None => ladder.push(live),
            }
        }
    }

    fn apply_remove(&mut self, world: i64, listings: &[WsListing]) {
        let Some(ladder) = self.listings.get_mut(&world) else {
            return;
        };
        for l in listings {
            let rec = listing_rec(l.price_per_unit, l.quantity, l.hq);
            let pos = match &l.listing_id {
                Some(id) => ladder.iter().position(|x| x.id.as_ref() == Some(id)),
                None => ladder.iter().position(|x| x.rec == rec),
            };
            if let Some(i) = pos {
                ladder.swap_remove(i);
            }
        }
    }

    fn apply_sales(&mut self, sales: &[WsSale]) {
        self.home_sales.extend(sales.iter().map(|s| SaleRec {
            price_per_unit: s.price_per_unit.max(0) as u64,
            quantity: s.quantity.clamp(0, i64::from(u32::MAX)) as u32,
            hq: s.hq,
            timestamp: s.timestamp,
        }));
    }
}

/// How the best opportunity moved between two computations.
fn classify(prev: Option<f64>, next: Option<f64>) -> Option<Change> {
    match (prev, next) {
        (None, Some(_)) => Some(Change::New),
        (Some(_), None) => Some(Change::Down),
        (Some(p), Some(n)) if n >= p * (1.0 + CHANGE_RATIO) => Some(Change::Up),
        (Some(p), Some(n)) if n <= p * (1.0 - CHANGE_RATIO) => Some(Change::Down),
        _ => None,
    }
}

fn should_notify(
    change: Option<Change>,
    profit: Option<f64>,
    threshold: i64,
    last: Option<i64>,
    now: i64,
) -> bool {
    matches!(change, Some(Change::New | Change::Up))
        && profit.is_some_and(|p| p >= threshold as f64)
        && last.is_none_or(|t| now - t >= NOTIFY_COOLDOWN_MS)
}

// ---------- views ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BestView {
    pub world: String,
    pub dc: String,
    #[serde(flatten)]
    pub source: SourceOpportunity,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchView {
    pub item_id: u32,
    pub item: Option<ItemInfo>,
    pub added_at: i64,
    pub notify_above_profit: Option<i64>,
    pub loaded: bool,
    pub error: Option<String>,
    pub best: Option<BestView>,
    pub home_min_nq: Option<u64>,
    pub home_min_hq: Option<u64>,
    pub change: Option<Change>,
    pub changed_at: Option<i64>,
    pub last_event_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum WsStatus {
    /// Nothing to watch, no connection.
    Idle,
    Connecting,
    Connected {
        subscriptions: usize,
    },
    #[serde(rename_all = "camelCase")]
    Disconnected {
        retry_at_ms: i64,
        error: String,
    },
}

#[derive(Debug)]
pub enum Cmd {
    Add(u32),
    Remove(u32),
    /// Settings changed: rebuild the scope and re-snapshot everything.
    Reload,
}

pub struct WatchManager {
    items: Mutex<HashMap<u32, ItemState>>,
    status: Mutex<WsStatus>,
    tx: mpsc::UnboundedSender<Cmd>,
}

impl WatchManager {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<Cmd>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let m = Self {
            items: Mutex::new(HashMap::new()),
            status: Mutex::new(WsStatus::Idle),
            tx,
        };
        (m, rx)
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }

    pub fn status(&self) -> WsStatus {
        self.status.lock().expect("watch lock").clone()
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WatchRow {
    item_id: i64,
    added_at: i64,
    notify_above_profit: Option<i64>,
}

async fn rows(state: &AppState) -> AppResult<Vec<WatchRow>> {
    Ok(sqlx::query_as(
        "SELECT item_id, added_at, notify_above_profit FROM watchlist ORDER BY added_at DESC",
    )
    .fetch_all(&state.db)
    .await?)
}

fn view(row: &WatchRow, st: Option<&ItemState>, scope: Option<&Scope>) -> WatchView {
    let home = scope.map(|s| s.home.id);
    let home_min = |q: Quality| {
        let ladder = st?.listings.get(&home?)?;
        ladder
            .iter()
            .filter(|l| l.rec.hq == q.is_hq())
            .map(|l| l.rec.price_per_unit)
            .min()
    };
    WatchView {
        item_id: row.item_id as u32,
        item: st.and_then(|s| s.info.clone()),
        added_at: row.added_at,
        notify_above_profit: row.notify_above_profit,
        loaded: st.is_some_and(|s| s.loaded),
        error: st.and_then(|s| s.error.clone()),
        best: st.and_then(|s| s.best.clone()).map(|source| {
            let w = scope.and_then(|s| s.world(source.world_id));
            BestView {
                world: w.map_or_else(|| source.world_id.to_string(), |w| w.name.clone()),
                dc: w.map(|w| w.dc.clone()).unwrap_or_default(),
                source,
            }
        }),
        home_min_nq: home_min(Quality::NQ),
        home_min_hq: home_min(Quality::HQ),
        change: st.and_then(|s| s.change),
        changed_at: st.and_then(|s| s.changed_at),
        last_event_ms: st.and_then(|s| s.last_event_ms),
    }
}

pub async fn list(state: &AppState) -> AppResult<Vec<WatchView>> {
    let settings = settings::load(&state.db).await?;
    let scope = scope::build(&settings, &worlds::load(&state.db).await?).ok();
    let rows = rows(state).await?;
    let missing: Vec<u32> = {
        let items = state.watch.items.lock().expect("watch lock");
        rows.iter()
            .map(|r| r.item_id as u32)
            .filter(|id| items.get(id).is_none_or(|s| s.info.is_none()))
            .collect()
    };
    // Names for items not yet snapshotted (e.g. just added, offline).
    let infos = item_infos(&state.xivapi, &missing, settings.language).await;
    let items = state.watch.items.lock().expect("watch lock");
    Ok(rows
        .iter()
        .map(|r| {
            let mut v = view(r, items.get(&(r.item_id as u32)), scope.as_ref());
            if v.item.is_none() {
                v.item = infos.get(&(r.item_id as u32)).cloned();
            }
            v
        })
        .collect())
}

pub async fn add(state: &AppState, item_id: u32) -> AppResult<()> {
    sqlx::query("INSERT OR IGNORE INTO watchlist (item_id, added_at) VALUES (?, ?)")
        .bind(item_id)
        .bind(now_ms())
        .execute(&state.db)
        .await?;
    state.watch.send(Cmd::Add(item_id));
    Ok(())
}

pub async fn remove(state: &AppState, item_id: u32) -> AppResult<()> {
    sqlx::query("DELETE FROM watchlist WHERE item_id = ?")
        .bind(item_id)
        .execute(&state.db)
        .await?;
    state
        .watch
        .items
        .lock()
        .expect("watch lock")
        .remove(&item_id);
    state.watch.send(Cmd::Remove(item_id));
    Ok(())
}

pub async fn set_threshold(state: &AppState, item_id: u32, profit: Option<i64>) -> AppResult<()> {
    if profit.is_some_and(|p| p < 0) {
        return Err(AppError::Invalid("threshold must be >= 0".into()));
    }
    sqlx::query("UPDATE watchlist SET notify_above_profit = ? WHERE item_id = ?")
        .bind(profit)
        .bind(item_id)
        .execute(&state.db)
        .await?;
    Ok(())
}

// ---------- the live loop ----------

/// Everything a recomputation needs, rebuilt on (re)connect and settings change.
struct Ctx {
    settings: Settings,
    scope: Scope,
    seller_tax: f64,
}

async fn build_ctx(state: &AppState) -> AppResult<Ctx> {
    let settings = settings::load(&state.db).await?;
    let scope = scope::build(&settings, &worlds::load(&state.db).await?)?;
    let seller_tax = scan::seller_tax(&state.universalis, &scope, &settings).await;
    Ok(Ctx {
        settings,
        scope,
        seller_tax,
    })
}

fn set_status(app: &AppHandle, status: WsStatus) {
    let state = app.state::<AppState>();
    let mut cur = state.watch.status.lock().expect("watch lock");
    if *cur != status {
        *cur = status.clone();
        let _ = app.emit(EVENT_WS_STATUS, status);
    }
}

/// Recomputes F5 for one item, classifies the change, notifies, emits.
async fn recompute(app: &AppHandle, ctx: &Ctx, item_id: u32, baseline: bool) {
    let state = app.state::<AppState>();
    let row = match sqlx::query_as::<_, WatchRow>(
        "SELECT item_id, added_at, notify_above_profit FROM watchlist WHERE item_id = ?",
    )
    .bind(item_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(r)) => r,
        _ => return, // removed meanwhile
    };
    let now = now_ms();
    let (view, notify) = {
        let mut items = state.watch.items.lock().expect("watch lock");
        let Some(st) = items.get_mut(&item_id) else {
            return;
        };
        let since = now / 1000 - PARAMS.history_window_secs;
        st.home_sales.retain(|s| s.timestamp >= since);
        let ladders: BTreeMap<i64, Vec<ListingRec>> = st
            .listings
            .iter()
            .map(|(w, l)| (*w, l.iter().map(|x| x.rec).collect()))
            .collect();
        let home_listings = ladders.get(&ctx.scope.home.id).cloned().unwrap_or_default();
        let ictx = ItemContext {
            home_world_id: ctx.scope.home.id,
            home_sales: &st.home_sales,
            home_listings: &home_listings,
            seller_tax: ctx.seller_tax,
            sell_horizon_days: f64::from(ctx.settings.sell_horizon_days),
            max_data_age_ms: i64::from(ctx.settings.max_data_age_hours) * 3600 * 1000,
            now_ms: now,
        };
        let best = analysis::analyze_item(&ictx, &ladders, &st.uploads, &PARAMS)
            .sources
            .into_iter()
            .next();
        let prev = st.best.as_ref().map(|b| b.eval.profit_per_trip);
        let next = best.as_ref().map(|b| b.eval.profit_per_trip);
        let change = if baseline { None } else { classify(prev, next) };
        let threshold = row
            .notify_above_profit
            .unwrap_or(ctx.settings.watch_notify_profit);
        let notify = ctx.settings.notifications_enabled
            && should_notify(change, next, threshold, st.last_notified_ms, now);
        if change.is_some() {
            st.change = change;
            st.changed_at = Some(now);
        }
        if notify {
            st.last_notified_ms = Some(now);
        }
        st.best = best;
        (view(&row, Some(st), Some(&ctx.scope)), notify)
    };
    if notify {
        if let (Some(item), Some(best)) = (&view.item, &view.best) {
            let body = format!(
                "Buy {} on {} at {} gil → {} gil profit per trip",
                best.source.eval.units_to_buy,
                best.world,
                best.source.eval.source_unit_price,
                best.source.eval.profit_per_trip.round()
            );
            if let Err(e) = app
                .notification()
                .builder()
                .title(&item.name)
                .body(body)
                .show()
            {
                tracing::warn!(error = %e, "notification failed");
            }
        }
    }
    let _ = app.emit(EVENT_WATCH_UPDATE, view);
}

/// Replaces an item's listings and home sales with fresh REST data.
async fn snapshot(app: &AppHandle, ctx: &Ctx, item_id: u32) {
    let state = app.state::<AppState>();
    let api = &state.universalis;
    let ids = [item_id];
    let markets = futures::future::join_all(
        ctx.scope
            .dcs
            .iter()
            .map(|dc| api.market(&dc.name, &ids, None, CachePolicy::Refresh)),
    );
    let history = api.history(
        &ctx.scope.home.name,
        &ids,
        HOME_HISTORY_WINDOW,
        CachePolicy::Refresh,
    );
    let info = item_infos(&state.xivapi, &ids, ctx.settings.language);
    let (markets, history, info) = tokio::join!(markets, history, info);

    let mut listings: BTreeMap<i64, Vec<LiveListing>> = BTreeMap::new();
    let mut uploads = HashMap::new();
    let mut error = None;
    for res in markets {
        match res {
            Ok(mut m) => {
                if let Some(item) = m.remove(&item_id) {
                    for l in &item.listings {
                        if let Some(w) = l.world_id {
                            listings.entry(w).or_default().push(LiveListing {
                                id: l.listing_id.clone(),
                                rec: listing_rec(
                                    l.price_per_unit as i64,
                                    i64::from(l.quantity),
                                    l.hq,
                                ),
                            });
                        }
                    }
                    uploads.extend(analysis::upload_times(&item));
                }
            }
            Err(e) => error = Some(e.to_string()),
        }
    }
    let home_sales = match history {
        Ok(mut h) => h
            .remove(&item_id)
            .map(|h| analysis::sales_of(&h))
            .unwrap_or_default(),
        Err(e) => {
            error = Some(e.to_string());
            Vec::new()
        }
    };
    let baseline = {
        let mut items = state.watch.items.lock().expect("watch lock");
        let st = items.entry(item_id).or_default();
        let baseline = !st.loaded;
        st.info = info.get(&item_id).cloned().or(st.info.take());
        st.listings = listings;
        st.uploads = uploads;
        st.home_sales = home_sales;
        st.loaded = true;
        st.error = error;
        baseline
    };
    recompute(app, ctx, item_id, baseline).await;
}

async fn snapshot_all(app: &AppHandle, ctx: &Ctx, ids: &[u32]) {
    futures::stream::iter(ids.iter().copied())
        .for_each_concurrent(4, |id| snapshot(app, ctx, id))
        .await;
}

async fn handle(app: &AppHandle, ctx: &Ctx, msg: ServerMsg) {
    if msg.event == "subscribe/error" {
        tracing::warn!(reason = ?msg.reason, "WebSocket subscribe error");
        return;
    }
    let (Some(item), Some(world)) = (msg.item, msg.world) else {
        return;
    };
    let item_id = item as u32;
    let in_scope = ctx.scope.world(world).is_some();
    let is_home = world == ctx.scope.home.id;
    if !in_scope {
        return;
    }
    let now = now_ms();
    {
        let state = app.state::<AppState>();
        let mut items = state.watch.items.lock().expect("watch lock");
        let Some(st) = items.get_mut(&item_id) else {
            return;
        };
        match msg.event.as_str() {
            "listings/add" => st.apply_add(world, &msg.listings),
            "listings/remove" => st.apply_remove(world, &msg.listings),
            "sales/add" if is_home => st.apply_sales(&msg.sales),
            "sales/add" => {}
            other => {
                tracing::debug!(event = other, "ignored WebSocket event");
                return;
            }
        }
        st.uploads.insert(world, now);
        st.last_event_ms = Some(now);
    }
    tracing::debug!(item_id, world, event = %msg.event, "live event applied");
    recompute(app, ctx, item_id, false).await;
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

enum SessionEnd {
    /// Watchlist became empty or the app is shutting down.
    Idle,
    /// The command channel closed.
    Shutdown,
}

async fn subscribe(ws: &mut Ws, event: &str, item: u32) -> AppResult<()> {
    for ch in channels(item) {
        ws.send(encode(event, ch)?)
            .await
            .map_err(|e| AppError::Other(format!("WebSocket send: {e}")))?;
    }
    Ok(())
}

async fn session(
    app: &AppHandle,
    ws: &mut Ws,
    rx: &mut mpsc::UnboundedReceiver<Cmd>,
) -> AppResult<SessionEnd> {
    let state = app.state::<AppState>();
    let mut ctx = build_ctx(&state).await?;
    let mut ids: Vec<u32> = rows(&state)
        .await?
        .iter()
        .map(|r| r.item_id as u32)
        .collect();
    for &id in &ids {
        subscribe(ws, "subscribe", id).await?;
    }
    set_status(
        app,
        WsStatus::Connected {
            subscriptions: ids.len() * 2,
        },
    );
    snapshot_all(app, &ctx, &ids).await;

    let mut refresh = tokio::time::interval(SNAPSHOT_REFRESH);
    refresh.tick().await; // the first tick fires immediately
    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.tick().await;
    loop {
        tokio::select! {
            _ = ping.tick() => {
                ws.send(Message::Ping(Vec::new().into()))
                    .await
                    .map_err(|e| AppError::Other(format!("WebSocket ping: {e}")))?;
            }
            msg = ws.next() => match msg {
                Some(Ok(Message::Binary(bytes))) => match decode(&bytes) {
                    Ok(m) => handle(app, &ctx, m).await,
                    Err(e) => tracing::warn!(error = %e, "bad WebSocket message"),
                },
                // Tungstenite queues the pong; flushing sends it.
                Some(Ok(Message::Ping(_))) => {
                    let _ = ws.flush().await;
                }
                Some(Ok(Message::Close(frame))) => {
                    return Err(AppError::Other(format!("server closed the connection: {frame:?}")));
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => return Err(AppError::Other(format!("WebSocket: {e}"))),
                None => return Err(AppError::Other("WebSocket stream ended".into())),
            },
            cmd = rx.recv() => match cmd {
                Some(Cmd::Add(id)) => {
                    if !ids.contains(&id) {
                        ids.push(id);
                        subscribe(ws, "subscribe", id).await?;
                    }
                    set_status(app, WsStatus::Connected { subscriptions: ids.len() * 2 });
                    snapshot(app, &ctx, id).await;
                }
                Some(Cmd::Remove(id)) => {
                    ids.retain(|&x| x != id);
                    subscribe(ws, "unsubscribe", id).await?;
                    if ids.is_empty() {
                        let _ = ws.close(None).await;
                        return Ok(SessionEnd::Idle);
                    }
                    set_status(app, WsStatus::Connected { subscriptions: ids.len() * 2 });
                }
                Some(Cmd::Reload) => {
                    ctx = build_ctx(&state).await?;
                    snapshot_all(app, &ctx, &ids).await;
                }
                None => return Ok(SessionEnd::Shutdown),
            },
            _ = refresh.tick() => snapshot_all(app, &ctx, &ids).await,
        }
    }
}

/// Owns the WebSocket connection for the app's lifetime: connects while the
/// watchlist is non-empty, reconnects with exponential backoff.
pub async fn run(app: AppHandle, mut rx: mpsc::UnboundedReceiver<Cmd>) {
    let mut backoff = Duration::from_secs(1);
    loop {
        let state = app.state::<AppState>();
        let watched = match rows(&state).await {
            Ok(r) => !r.is_empty(),
            Err(e) => {
                tracing::error!(error = %e, "watchlist unreadable");
                false
            }
        };
        let home_set = settings::load(&state.db)
            .await
            .is_ok_and(|s| s.home_world_id.is_some());
        if !watched || !home_set {
            set_status(&app, WsStatus::Idle);
            match rx.recv().await {
                Some(_) => continue,
                None => return,
            }
        }

        set_status(&app, WsStatus::Connecting);
        let error = match tokio_tungstenite::connect_async(WS_URL).await {
            Ok((mut ws, _)) => {
                tracing::info!("WebSocket connected");
                backoff = Duration::from_secs(1);
                match session(&app, &mut ws, &mut rx).await {
                    Ok(SessionEnd::Idle) => continue,
                    Ok(SessionEnd::Shutdown) => return,
                    Err(e) => e.to_string(),
                }
            }
            Err(e) => format!("connect: {e}"),
        };
        tracing::warn!(%error, ?backoff, "WebSocket down, reconnecting");
        set_status(
            &app,
            WsStatus::Disconnected {
                retry_at_ms: now_ms() + backoff.as_millis() as i64,
                error,
            },
        );
        // Wait out the backoff, but still accept commands (they are re-read
        // from the database on reconnect).
        let sleep = tokio::time::sleep(backoff);
        tokio::pin!(sleep);
        loop {
            tokio::select! {
                _ = &mut sleep => break,
                cmd = rx.recv() => if cmd.is_none() { return },
            }
        }
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wl(id: &str, price: i64, qty: i64) -> WsListing {
        WsListing {
            listing_id: Some(id.into()),
            price_per_unit: price,
            quantity: qty,
            hq: false,
        }
    }

    #[test]
    fn listings_add_and_remove_by_id() {
        let mut st = ItemState::default();
        st.apply_add(33, &[wl("a", 100, 1), wl("b", 200, 2)]);
        st.apply_add(33, &[wl("a", 90, 1)]); // same id: replaced, not duplicated
        assert_eq!(st.listings[&33].len(), 2);
        assert_eq!(
            st.listings[&33]
                .iter()
                .find(|l| l.id.as_deref() == Some("a"))
                .unwrap()
                .rec
                .price_per_unit,
            90
        );
        st.apply_remove(33, &[wl("b", 200, 2)]);
        assert_eq!(st.listings[&33].len(), 1);
        st.apply_remove(42, &[wl("a", 90, 1)]); // other world: no effect
        assert_eq!(st.listings[&33].len(), 1);
    }

    #[test]
    fn change_classification() {
        assert_eq!(classify(None, Some(1.0)), Some(Change::New));
        assert_eq!(classify(Some(100.0), None), Some(Change::Down));
        assert_eq!(classify(Some(100.0), Some(115.0)), Some(Change::Up));
        assert_eq!(classify(Some(100.0), Some(85.0)), Some(Change::Down));
        assert_eq!(classify(Some(100.0), Some(105.0)), None);
        assert_eq!(classify(None, None), None);
    }

    #[test]
    fn notification_rules() {
        let now = 10 * NOTIFY_COOLDOWN_MS;
        assert!(should_notify(
            Some(Change::New),
            Some(200.0),
            100,
            None,
            now
        ));
        assert!(!should_notify(
            Some(Change::Down),
            Some(200.0),
            100,
            None,
            now
        ));
        assert!(
            !should_notify(Some(Change::Up), Some(50.0), 100, None, now),
            "below threshold"
        );
        assert!(
            !should_notify(Some(Change::Up), Some(200.0), 100, Some(now - 1000), now),
            "cooldown"
        );
        assert!(should_notify(
            Some(Change::Up),
            Some(200.0),
            100,
            Some(now - NOTIFY_COOLDOWN_MS),
            now
        ));
    }

    #[test]
    fn bson_round_trip() {
        let msg = encode("subscribe", channels(5333)[0].clone()).unwrap();
        let Message::Binary(bytes) = msg else {
            panic!("binary expected")
        };
        let doc: bson::Document = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(doc.get_str("event").unwrap(), "subscribe");
        assert_eq!(doc.get_str("channel").unwrap(), "listings{item=5333}");

        // Server event, with Int32 fields as the .NET serializer writes them.
        let event = bson::doc! {
            "event": "listings/add", "item": 5333_i32, "world": 33_i32,
            "listings": [{ "listingID": "x1", "pricePerUnit": 12_i32, "quantity": 99_i32, "hq": false,
                           "worldName": "Twintania", "retainerName": "R" }],
        };
        let m = decode(&event.to_vec().unwrap()).unwrap();
        assert_eq!(
            (m.event.as_str(), m.item, m.world),
            ("listings/add", Some(5333), Some(33))
        );
        assert_eq!(m.listings[0].price_per_unit, 12);
        assert_eq!(m.listings[0].listing_id.as_deref(), Some("x1"));
    }
}

/// Live checks against Universalis:
/// `cargo test live_ws -- --ignored --nocapture`
#[cfg(test)]
mod live {
    use super::*;

    /// Hierarchical channel + filter delivers both add and remove events.
    #[tokio::test]
    #[ignore = "hits the network"]
    async fn live_ws_events() {
        let (mut ws, _) = tokio_tungstenite::connect_async(WS_URL).await.unwrap();
        // A busy world (Chinese DC) so events arrive within seconds.
        ws.send(encode("subscribe", "listings{world=1172}".into()).unwrap())
            .await
            .unwrap();
        // World traffic is bursty: one event within a minute is enough.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        loop {
            let msg = tokio::time::timeout_at(deadline, ws.next())
                .await
                .expect("no event within 60s");
            if let Some(Ok(Message::Binary(b))) = msg {
                let m = decode(&b).unwrap();
                println!(
                    "{} item={:?} world={:?} listings={}",
                    m.event,
                    m.item,
                    m.world,
                    m.listings.len()
                );
                assert_eq!(m.world, Some(1172));
                assert!(m.event.starts_with("listings/"));
                break;
            }
            assert!(
                !matches!(msg, Some(Err(_)) | None),
                "connection lost: {msg:?}"
            );
        }
    }

    /// With our pings, a quiet per-item subscription survives the proxy's 60s idle cut.
    #[tokio::test]
    #[ignore = "hits the network for ~80s"]
    async fn live_ws_keepalive() {
        let (mut ws, _) = tokio_tungstenite::connect_async(WS_URL).await.unwrap();
        subscribe(&mut ws, "subscribe", 2).await.unwrap();
        let end = tokio::time::Instant::now() + Duration::from_secs(80);
        let mut ping = tokio::time::interval(PING_INTERVAL);
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(end) => break,
                _ = ping.tick() => ws.send(Message::Ping(Vec::new().into())).await.unwrap(),
                msg = ws.next() => match msg {
                    Some(Ok(_)) => {}
                    other => panic!("connection lost: {other:?}"),
                },
            }
        }
        println!("still connected after 80s");
    }
}
