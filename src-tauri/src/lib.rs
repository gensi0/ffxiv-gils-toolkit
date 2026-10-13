mod analysis;
mod commands;
mod db;
mod detail;
mod error;
mod logging;
mod net;
mod profit;
mod results;
mod scan;
mod scope;
mod settings;
mod spender;
mod stats;
mod universalis;
mod ventes;
mod watch;
mod worlds;
mod xivapi;

use std::path::PathBuf;
use std::time::Duration;

use sqlx::SqlitePool;
use tauri::http::{header, Response, StatusCode};
use tauri::{AppHandle, Emitter, Manager};
use tracing_appender::non_blocking::WorkerGuard;

/// Emitted when the world list in SQLite changed after a background refresh.
pub const EVENT_WORLDS_UPDATED: &str = "worlds-updated";
/// Emitted with a [`net::NetStatus`] whenever the Universalis client state changes.
pub const EVENT_NET_STATUS: &str = "net-status";

pub struct AppState {
    pub db: SqlitePool,
    pub universalis: universalis::Universalis,
    pub xivapi: xivapi::XivApi,
    pub scans: scan::ScanManager,
    pub watch: watch::WatchManager,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    _log_guard: Option<WorkerGuard>,
}

/// Percent-decodes a URI path segment (as produced by `convertFileSrc`).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Background jobs: world refresh, cache pruning, network status relay and
/// the scan scheduler.
fn spawn_background(app: &AppHandle, http: reqwest::Client) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = handle.state::<AppState>();
        match worlds::refresh(&state.db, &http).await {
            Ok(true) => {
                let _ = handle.emit(EVENT_WORLDS_UPDATED, ());
            }
            Ok(false) => {}
            Err(e) => tracing::warn!(error = %e, "world refresh failed, keeping cached list"),
        }
        match net::cache_prune(&state.db).await {
            Ok(n) if n > 0 => tracing::info!(rows = n, "pruned API cache"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "cache prune failed"),
        }
    });

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut rx = handle.state::<AppState>().universalis.net().subscribe();
        while rx.changed().await.is_ok() {
            let status = rx.borrow_and_update().clone();
            let _ = handle.emit(EVENT_NET_STATUS, status);
        }
    });

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            if let Err(e) = maybe_scheduled_scan(&handle).await {
                tracing::warn!(error = %e, "scan scheduler");
            }
        }
    });
}

async fn maybe_scheduled_scan(app: &AppHandle) -> error::AppResult<()> {
    let state = app.state::<AppState>();
    let s = settings::load(&state.db).await?;
    if s.scan_interval_hours == 0 || s.home_world_id.is_none() || state.scans.is_running() {
        return Ok(());
    }
    let last: Option<i64> = sqlx::query_scalar("SELECT MAX(started_at) FROM scan_runs")
        .fetch_one(&state.db)
        .await?;
    let due =
        last.is_none_or(|t| net::now_ms() - t >= i64::from(s.scan_interval_hours) * 3_600_000);
    if due {
        tracing::info!("starting scheduled scan");
        scan::start(app)?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .register_asynchronous_uri_scheme_protocol("icon", |ctx, request, responder| {
            // icon://localhost/<percent-encoded XIVAPI icon path>
            let app = ctx.app_handle().clone();
            let path = percent_decode(request.uri().path().trim_start_matches('/'));
            tauri::async_runtime::spawn(async move {
                let res = app.state::<AppState>().xivapi.icon(&path).await;
                let response = match res {
                    Ok(bytes) => Response::builder()
                        .header(header::CONTENT_TYPE, "image/png")
                        .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
                        .body(bytes),
                    Err(e) => {
                        tracing::debug!(%path, error = %e, "icon unavailable");
                        Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(Vec::new())
                    }
                };
                responder.respond(response.expect("valid response"));
            });
        })
        .setup(|app| {
            let paths = app.path();
            let data_dir = paths.app_data_dir()?;
            let log_dir = paths.app_log_dir()?;
            let cache_dir = paths.app_cache_dir()?;
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
                .user_agent(concat!("ffxiv-gils-toolkit/", env!("CARGO_PKG_VERSION")))
                .gzip(true)
                .build()?;

            let data_dir_for_spender = data_dir.join("donnees-jeu.json");
            let (watch, watch_rx) = watch::WatchManager::new();
            app.manage(AppState {
                universalis: universalis::Universalis::new(
                    net::Throttled::new(http.clone()),
                    db.clone(),
                ),
                xivapi: xivapi::XivApi::new(http.clone(), db.clone(), cache_dir.join("icons")),
                scans: scan::ScanManager::default(),
                watch,
                db,
                data_dir,
                log_dir,
                _log_guard: log_guard,
            });
            app.manage(spender::SpenderState::new(data_dir_for_spender));
            spawn_background(app.handle(), http);
            tauri::async_runtime::spawn(watch::run(app.handle().clone(), watch_rx));
            tauri::async_runtime::spawn(ventes::run(app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_worlds,
            commands::get_settings,
            commands::tax_rates,
            commands::save_settings,
            commands::app_info,
            commands::open_log_dir,
            commands::net_status,
            commands::search_items,
            commands::item_detail,
            commands::start_scan,
            commands::retry_failed_scan,
            commands::cancel_scan,
            commands::scan_progress,
            commands::list_scans,
            commands::scan_results,
            commands::scan_candidates,
            commands::watchlist_list,
            commands::watchlist_add,
            commands::watchlist_remove,
            commands::watchlist_set_threshold,
            commands::ws_status,
            commands::market_stats,
            commands::ventes_overview,
            commands::ventes_refresh,
            commands::ventes_set_price,
            commands::ventes_close,
            commands::ventes_track,
            commands::journal_list,
            commands::journal_add_purchase,
            commands::journal_delete_purchase,
            commands::journal_add_sale,
            commands::journal_delete_sale,
            spender::spender_dataset_status,
            spender::spender_build_dataset,
            spender::spender_currencies,
            spender::spender_offers,
            spender::spender_gil_offers,
            spender::spender_deliveries,
            spender::spender_ventures,
            spender::spender_desynths,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application");
}

#[cfg(test)]
mod tests {
    #[test]
    fn percent_decode_icon_paths() {
        assert_eq!(
            super::percent_decode("ui%2Ficon%2F020000%2F020801_hr1.tex"),
            "ui/icon/020000/020801_hr1.tex"
        );
        assert_eq!(super::percent_decode("a%2"), "a%2");
    }
}
