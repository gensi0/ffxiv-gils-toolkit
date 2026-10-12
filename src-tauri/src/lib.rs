mod commands;
mod db;
mod error;
mod logging;
mod settings;
mod worlds;

use std::path::PathBuf;

use sqlx::SqlitePool;
use tauri::{Emitter, Manager};
use tracing_appender::non_blocking::WorkerGuard;

/// Emitted when the world list in SQLite changed after a background refresh.
pub const EVENT_WORLDS_UPDATED: &str = "worlds-updated";

pub struct AppState {
    pub db: SqlitePool,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    _log_guard: Option<WorkerGuard>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let paths = app.path();
            let data_dir = paths.app_data_dir()?;
            let log_dir = paths.app_log_dir()?;
            std::fs::create_dir_all(&data_dir)?;

            let log_guard = logging::init(&log_dir)
                .inspect_err(|e| eprintln!("file logging disabled: {e}"))
                .ok();
            tracing::info!(
                version = %app.package_info().version,
                data_dir = %data_dir.display(),
                "starting"
            );

            let db = tauri::async_runtime::block_on(async {
                let pool = db::open(&data_dir.join("ffxiv-trader.db")).await?;
                worlds::ensure_seeded(&pool).await?;
                Ok::<_, error::AppError>(pool)
            })?;
            let http = reqwest::Client::builder()
                .user_agent(concat!("ffxiv-trader/", env!("CARGO_PKG_VERSION")))
                .gzip(true)
                .build()?;

            // Refresh the world list in the background; the bundled copy
            // (or the previous run's list) is served meanwhile.
            let handle = app.handle().clone();
            let bg_db = db.clone();
            tauri::async_runtime::spawn(async move {
                match worlds::refresh(&bg_db, &http).await {
                    Ok(true) => {
                        let _ = handle.emit(EVENT_WORLDS_UPDATED, ());
                    }
                    Ok(false) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "world refresh failed, keeping cached list")
                    }
                }
            });

            app.manage(AppState {
                db,
                data_dir,
                log_dir,
                _log_guard: log_guard,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_worlds,
            commands::get_settings,
            commands::save_settings,
            commands::app_info,
            commands::open_log_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
