use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{AppError, AppResult};
use crate::settings::{self, Settings};
use crate::worlds::{self, World};
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
