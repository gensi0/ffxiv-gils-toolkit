//! Serveur FFXIV Gils Toolkit : API pour le client web (PWA), comptes,
//! tâches de fond par compte (suivi des ventes, scans programmés) et fichiers
//! statiques du client.
//!
//! Configuration par variables d'environnement :
//! - `DATA_DIR` (défaut `/data`) : bases SQLite, caches, icônes ;
//! - `WEB_DIR` (défaut `/app/web`) : client web compilé ;
//! - `PORT` (défaut 8080) ;
//! - `INVITE_CODE` : code demandé à l'inscription (inscriptions fermées sinon) ;
//! - `COOKIE_SECURE` (défaut `true`) : `false` pour tester en HTTP.

mod analysis;
mod app;
mod auth;
mod commands;
mod craft;
mod db;
mod detail;
mod error;
mod net;
mod plugin;
mod profit;
mod push;
mod results;
mod rpc;
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
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;
use tower_http::compression::CompressionLayer;
use tower_http::services::{ServeDir, ServeFile};

pub use app::AppState;
use auth::{ApiError, User};

pub struct Server {
    pub shared: Arc<app::Shared>,
    pub web_dir: PathBuf,
    pub invite_code: Option<String>,
    pub cookie_secure: bool,
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match argv.first().map(String::as_str) {
        Some("--healthcheck") => {
            let port = env_or("PORT", "8080");
            let ok = reqwest::get(format!("http://127.0.0.1:{port}/api/health"))
                .await
                .is_ok_and(|r| r.status().is_success());
            std::process::exit(i32::from(!ok));
        }
        Some("--backup") => {
            let dest = PathBuf::from(argv.get(1).map_or("/data/backup", String::as_str));
            let n = db::backup(&PathBuf::from(env_or("DATA_DIR", "/data")), &dest).await?;
            println!("{n} bases copiées dans {}", dest.display());
            return Ok(());
        }
        Some(other) => {
            eprintln!("option inconnue : {other} (options : --healthcheck, --backup [dossier])");
            std::process::exit(2);
        }
        None => {}
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    let data_dir = PathBuf::from(env_or("DATA_DIR", "/data"));
    let web_dir = PathBuf::from(env_or("WEB_DIR", "/app/web"));
    let port: u16 = env_or("PORT", "8080").parse()?;
    std::fs::create_dir_all(&data_dir)?;

    let accounts = db::open_accounts(&data_dir.join("accounts.db")).await?;
    let shared = app::Shared::new(data_dir.clone(), accounts.clone()).await?;
    let server = Arc::new(Server {
        shared: shared.clone(),
        web_dir: web_dir.clone(),
        invite_code: std::env::var("INVITE_CODE")
            .ok()
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty()),
        cookie_secure: env_or("COOKIE_SECURE", "true") != "false",
    });
    if server.invite_code.is_none() {
        tracing::warn!("INVITE_CODE non défini : inscriptions fermées");
    }

    // Les tâches de fond de chaque compte (suivi des ventes…) tournent en
    // permanence, même sans client connecté.
    let users: Vec<(i64, String)> = sqlx::query_as("SELECT id, username FROM users")
        .fetch_all(&accounts)
        .await?;
    for (id, name) in users {
        if let Err(e) = shared.user(id, &name).await {
            tracing::error!(user_id = id, error = %e, "compte impossible à charger");
        }
    }

    let index = web_dir.join("index.html");
    let app = Router::new()
        .route("/api/auth/register", post(auth::register))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/password", post(auth::change_password))
        .route("/api/cmd/{name}", post(command))
        .route("/api/events", get(events))
        .route("/api/icon/{*path}", get(icon))
        .route("/api/push/key", get(push_key))
        .route("/api/push/subscribe", post(push_subscribe))
        .route("/api/push/unsubscribe", post(push_unsubscribe))
        .route("/api/push/test", post(push_test))
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/plugin/ping", get(plugin::ping))
        .route("/api/plugin/sync", post(plugin::sync))
        .route(
            "/api/plugin/tokens",
            get(plugin::list_tokens).post(plugin::create_token),
        )
        .route("/api/plugin/tokens/revoke", post(plugin::revoke_token))
        .route("/plugin/pluginmaster.json", get(plugin::pluginmaster))
        .route("/plugin/latest.zip", get(plugin::zip))
        .fallback_service(ServeDir::new(&web_dir).fallback(ServeFile::new(index)))
        .layer(CompressionLayer::new())
        .with_state(server);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, version = env!("CARGO_PKG_VERSION"), "serveur démarré");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

/// `POST /api/cmd/<nom>` : commandes de l'app (mêmes noms que sous Tauri),
/// plus le stockage clé-valeur du client.
async fn command(
    User(app): User,
    Path(name): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, ApiError> {
    let input = if body.is_empty() {
        Value::Object(serde_json::Map::new())
    } else {
        serde_json::from_slice(&body)
            .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("JSON invalide : {e}")))?
    };
    let res = match name.as_str() {
        "kv_get" => kv_get(&app, input).await,
        "kv_set" => kv_set(&app, input).await,
        "plugin_status" => plugin::status(&app).await,
        _ => rpc::dispatch(&app, &name, input).await,
    };
    res.map(Json)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))
}

#[derive(Deserialize)]
struct KvKey {
    key: String,
}

#[derive(Deserialize)]
struct KvSet {
    key: String,
    value: String,
}

async fn kv_get(app: &app::AppHandle, input: Value) -> Result<Value, String> {
    let k: KvKey = serde_json::from_value(input).map_err(|e| e.to_string())?;
    let v: Option<String> = sqlx::query_scalar("SELECT value FROM kv WHERE key = ?")
        .bind(&k.key)
        .fetch_optional(&app.state::<AppState>().db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(v.map_or(Value::Null, Value::String))
}

async fn kv_set(app: &app::AppHandle, input: Value) -> Result<Value, String> {
    let k: KvSet = serde_json::from_value(input).map_err(|e| e.to_string())?;
    if k.value.len() > 2_000_000 {
        return Err("valeur trop grande".into());
    }
    sqlx::query(
        "INSERT INTO kv (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(&k.key)
    .bind(&k.value)
    .bind(net::now_ms())
    .execute(&app.state::<AppState>().db)
    .await
    .map_err(|e| e.to_string())?;
    // Les autres appareils rechargent la donnée.
    let _ = app.emit("kv-update", &k.key);
    Ok(Value::Null)
}

/// `GET /api/events` : événements du compte (progression du scan, ventes…).
async fn events(User(app): User, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| relay(app, socket))
}

async fn relay(app: app::AppHandle, mut socket: WebSocket) {
    let mut rx = app.subscribe();
    let mut ping = tokio::time::interval(std::time::Duration::from_secs(30));
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Ok(text) => {
                    if socket.send(Message::Text(text.as_ref().into())).await.is_err() {
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::debug!(n, "événements perdus (client lent)");
                }
                Err(_) => return,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                Some(Ok(_)) => {}
            },
            _ = ping.tick() => {
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    return;
                }
            }
        }
    }
}

/// `GET /api/icon/<chemin XIVAPI>` : icônes mises en cache sur disque.
async fn icon(State(server): State<Arc<Server>>, Path(path): Path<String>) -> Response {
    match server.shared.xivapi.icon(&path).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            ],
            bytes,
        )
            .into_response(),
        Err(e) => {
            tracing::debug!(%path, error = %e, "icône indisponible");
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

async fn push_key(State(server): State<Arc<Server>>) -> Json<Value> {
    Json(serde_json::json!({ "key": server.shared.push.public_key() }))
}

async fn push_subscribe(
    State(server): State<Arc<Server>>,
    User(app): User,
    Json(sub): Json<push::Subscription>,
) -> Result<StatusCode, ApiError> {
    push::Push::subscribe(&server.shared.accounts, app.user_id(), &sub)
        .await
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct Endpoint {
    endpoint: String,
}

async fn push_unsubscribe(
    State(server): State<Arc<Server>>,
    User(app): User,
    Json(e): Json<Endpoint>,
) -> Result<StatusCode, ApiError> {
    push::Push::unsubscribe(&server.shared.accounts, app.user_id(), &e.endpoint).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Notification de test vers tous les appareils du compte.
async fn push_test(User(app): User) -> StatusCode {
    let _ = app
        .notification()
        .builder()
        .title("FFXIV Gils Toolkit")
        .body("Les notifications fonctionnent sur cet appareil.")
        .show();
    StatusCode::NO_CONTENT
}
