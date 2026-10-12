use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::detail::{self, ItemDetail};
use crate::error::{AppError, AppResult};
use crate::net::NetStatus;
use crate::results::{self, CandidateView, ScanResults, ScanRun};
use crate::scan::{self, ScanProgress};
use crate::settings::{self, Settings};
use crate::watch::{self, WatchView, WsStatus};
use crate::worlds::{self, World};
use crate::xivapi::SearchHit;
use crate::AppState;

#[tauri::command]
pub async fn list_worlds(state: State<'_, AppState>) -> AppResult<Vec<World>> {
    worlds::load(&state.db).await
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> AppResult<Settings> {
    settings::load(&state.db).await
}

#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, settings: Settings) -> AppResult<Settings> {
    let worlds = worlds::load(&state.db).await?;
    settings::save(&state.db, &settings, &worlds).await?;
    tracing::info!(home_world = ?settings.home_world_id, "settings saved");
    state.watch.send(watch::Cmd::Reload);
    settings::load(&state.db).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    data_dir: String,
    log_dir: String,
}

#[tauri::command]
pub fn app_info(app: AppHandle, state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        data_dir: state.data_dir.display().to_string(),
        log_dir: state.log_dir.display().to_string(),
    }
}

#[tauri::command]
pub fn open_log_dir(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    app.opener()
        .open_path(state.log_dir.display().to_string(), None::<&str>)
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub fn net_status(state: State<'_, AppState>) -> NetStatus {
    state.universalis.net().subscribe().borrow().clone()
}

#[tauri::command]
pub async fn search_items(state: State<'_, AppState>, text: String) -> AppResult<Vec<SearchHit>> {
    let lang = settings::load(&state.db).await?.language;
    state.xivapi.search(&text, lang, 30).await
}

#[tauri::command]
pub async fn item_detail(
    state: State<'_, AppState>,
    item_id: u32,
    refresh: bool,
) -> AppResult<ItemDetail> {
    detail::item_detail(&state, item_id, refresh).await
}

#[tauri::command]
pub fn start_scan(app: AppHandle) -> AppResult<String> {
    scan::start(&app)
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) -> bool {
    state.scans.cancel()
}

#[tauri::command]
pub fn scan_progress(state: State<'_, AppState>) -> Option<ScanProgress> {
    state.scans.current()
}

#[tauri::command]
pub async fn list_scans(state: State<'_, AppState>) -> AppResult<Vec<ScanRun>> {
    results::list_runs(&state.db, 100).await
}

#[tauri::command]
pub async fn scan_results(
    state: State<'_, AppState>,
    scan_id: Option<String>,
) -> AppResult<Option<ScanResults>> {
    let settings = settings::load(&state.db).await?;
    let worlds = worlds::load(&state.db).await?;
    results::scan_results(
        &state.db,
        &state.xivapi,
        &settings,
        &worlds,
        scan_id.as_deref(),
    )
    .await
}

#[tauri::command]
pub async fn scan_candidates(
    state: State<'_, AppState>,
    scan_id: String,
) -> AppResult<Vec<CandidateView>> {
    let settings = settings::load(&state.db).await?;
    let worlds = worlds::load(&state.db).await?;
    results::scan_candidates(&state.db, &state.xivapi, &settings, &worlds, &scan_id, 300).await
}

#[tauri::command]
pub async fn watchlist_list(state: State<'_, AppState>) -> AppResult<Vec<WatchView>> {
    watch::list(&state).await
}

#[tauri::command]
pub async fn watchlist_add(state: State<'_, AppState>, item_id: u32) -> AppResult<()> {
    watch::add(&state, item_id).await
}

#[tauri::command]
pub async fn watchlist_remove(state: State<'_, AppState>, item_id: u32) -> AppResult<()> {
    watch::remove(&state, item_id).await
}

#[tauri::command]
pub async fn watchlist_set_threshold(
    state: State<'_, AppState>,
    item_id: u32,
    profit: Option<i64>,
) -> AppResult<()> {
    watch::set_threshold(&state, item_id, profit).await
}

#[tauri::command]
pub fn ws_status(state: State<'_, AppState>) -> WsStatus {
    state.watch.status()
}

#[tauri::command]
pub async fn retry_failed_scan(app: AppHandle, scan_id: String) -> AppResult<()> {
    scan::retry_failed(&app, &scan_id).await
}
