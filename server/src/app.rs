//! Contexte du serveur. Chaque compte a son propre [`AppHandle`] : il remplace
//! celui de Tauri (mêmes méthodes `state`, `emit`, `notification`), si bien que
//! la logique métier reprise de l'application de bureau tourne telle quelle.
//!
//! Données : une base SQLite par compte (`users/<id>.db`, mêmes migrations que
//! l'app de bureau) et une base partagée (caches Universalis et XIVAPI,
//! communs à tous les comptes).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::{broadcast, Mutex};

use crate::craft::CraftState;
use crate::error::AppResult;
use crate::push::Push;
use crate::spender::SpenderState;
use crate::{db, net, scan, universalis, ventes, watch, worlds, xivapi};

pub const EVENT_WORLDS_UPDATED: &str = "worlds-updated";
pub const EVENT_NET_STATUS: &str = "net-status";

/// Ressources communes à tous les comptes.
pub struct Shared {
    pub data_dir: PathBuf,
    pub accounts: SqlitePool,
    pub http: reqwest::Client,
    pub universalis: universalis::Universalis,
    pub xivapi: xivapi::XivApi,
    pub spender: SpenderState,
    pub craft: CraftState,
    pub push: Push,
    users: Mutex<HashMap<i64, AppHandle>>,
}

impl Shared {
    pub async fn new(data_dir: PathBuf, accounts: SqlitePool) -> AppResult<Arc<Self>> {
        std::fs::create_dir_all(data_dir.join("users")).map_err(io)?;
        let shared_db = db::open(&data_dir.join("shared.db")).await?;
        let http = reqwest::Client::builder()
            .user_agent(concat!("ffxiv-gils-toolkit/", env!("CARGO_PKG_VERSION")))
            .gzip(true)
            .build()?;
        let push = Push::load(&accounts, http.clone()).await?;
        Ok(Arc::new(Self {
            universalis: universalis::Universalis::new(
                net::Throttled::new(http.clone()),
                shared_db.clone(),
            ),
            xivapi: xivapi::XivApi::new(http.clone(), shared_db.clone(), data_dir.join("icons")),
            spender: SpenderState::new(data_dir.join("donnees-jeu.json")),
            craft: CraftState::new(data_dir.join("recettes.json")),
            http,
            push,
            accounts,
            data_dir,
            users: Mutex::new(HashMap::new()),
        }))
        .inspect(|s| spawn_shared_tasks(s.clone(), shared_db))
    }

    /// Contexte d'un compte, créé (base, tâches de fond) au premier accès.
    pub async fn user(self: &Arc<Self>, user_id: i64, username: &str) -> AppResult<AppHandle> {
        let mut users = self.users.lock().await;
        if let Some(app) = users.get(&user_id) {
            return Ok(app.clone());
        }
        let app = AppHandle::open(self.clone(), user_id, username.to_string()).await?;
        users.insert(user_id, app.clone());
        Ok(app)
    }
}

fn io(e: std::io::Error) -> crate::error::AppError {
    crate::error::AppError::Other(format!("fichier : {e}"))
}

/// État d'un compte : mêmes champs que l'`AppState` de l'app de bureau.
pub struct AppState {
    pub user_id: i64,
    pub db: SqlitePool,
    pub universalis: universalis::Universalis,
    pub xivapi: xivapi::XivApi,
    pub scans: scan::ScanManager,
    pub watch: watch::WatchManager,
    pub ventes: ventes::Runtime,
    pub data_dir: PathBuf,
}

/// État d'un compte pour les tests (base en mémoire, sans tâches de fond).
#[cfg(test)]
pub fn test_state(db: SqlitePool) -> AppState {
    let http = reqwest::Client::new();
    AppState {
        user_id: 1,
        universalis: universalis::Universalis::new(net::Throttled::new(http.clone()), db.clone()),
        xivapi: xivapi::XivApi::new(
            http,
            db.clone(),
            std::env::temp_dir().join("gils-test-icons"),
        ),
        scans: scan::ScanManager::default(),
        watch: watch::WatchManager::new().0,
        ventes: ventes::Runtime::default(),
        data_dir: std::env::temp_dir(),
        db,
    }
}

pub struct UserApp {
    username: String,
    shared: Arc<Shared>,
    state: AppState,
    events: broadcast::Sender<Arc<str>>,
}

/// Remplace `tauri::AppHandle` : accès à l'état, événements vers les clients
/// connectés (WebSocket) et notifications (push).
#[derive(Clone)]
pub struct AppHandle(Arc<UserApp>);

/// Équivalent de `tauri::State` : une simple référence.
pub type State<'a, T> = &'a T;

/// Ce qu'on peut obtenir avec [`AppHandle::state`].
pub trait Managed {
    fn get(app: &UserApp) -> &Self;
}

impl Managed for AppState {
    fn get(app: &UserApp) -> &Self {
        &app.state
    }
}

impl Managed for SpenderState {
    fn get(app: &UserApp) -> &Self {
        &app.shared.spender
    }
}

impl Managed for CraftState {
    fn get(app: &UserApp) -> &Self {
        &app.shared.craft
    }
}

#[derive(Serialize)]
struct Envelope<'a, T> {
    event: &'a str,
    payload: T,
}

impl AppHandle {
    async fn open(shared: Arc<Shared>, user_id: i64, username: String) -> AppResult<Self> {
        let db = db::open(&shared.data_dir.join("users").join(format!("{user_id}.db"))).await?;
        worlds::ensure_seeded(&db).await?;
        let (watch, watch_rx) = watch::WatchManager::new();
        let (events, _) = broadcast::channel(256);
        let app = Self(Arc::new(UserApp {
            username,
            state: AppState {
                user_id,
                db,
                universalis: shared.universalis.clone(),
                xivapi: shared.xivapi.clone(),
                scans: scan::ScanManager::default(),
                watch,
                ventes: ventes::Runtime::default(),
                data_dir: shared.data_dir.clone(),
            },
            shared,
            events,
        }));
        app.spawn_tasks(watch_rx);
        tracing::info!(user_id, "compte chargé");
        Ok(app)
    }

    pub fn state<T: Managed>(&self) -> &T {
        T::get(&self.0)
    }

    pub fn shared(&self) -> &Arc<Shared> {
        &self.0.shared
    }

    pub fn username(&self) -> &str {
        &self.0.username
    }

    pub fn user_id(&self) -> i64 {
        self.0.state.user_id
    }

    /// Diffuse un événement aux clients connectés de ce compte.
    pub fn emit<S: Serialize>(&self, event: &str, payload: S) -> Result<(), serde_json::Error> {
        let msg = serde_json::to_string(&Envelope { event, payload })?;
        // Aucun client connecté : rien à faire.
        let _ = self.0.events.send(msg.into());
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<str>> {
        self.0.events.subscribe()
    }

    pub fn notification(&self) -> Notifier {
        Notifier(self.clone())
    }

    fn spawn_tasks(&self, watch_rx: tokio::sync::mpsc::UnboundedReceiver<watch::Cmd>) {
        tokio::spawn(watch::run(self.clone(), watch_rx));
        tokio::spawn(ventes::run(self.clone()));

        // Liste des serveurs, puis relais de l'état réseau.
        let app = self.clone();
        tokio::spawn(async move {
            let state = app.state::<AppState>();
            match worlds::refresh(&state.db, &app.shared().http).await {
                Ok(true) => {
                    let _ = app.emit(EVENT_WORLDS_UPDATED, ());
                }
                Ok(false) => {}
                Err(e) => tracing::warn!(error = %e, "liste des serveurs : mise à jour impossible"),
            }
            let mut rx = state.universalis.net().subscribe();
            while rx.changed().await.is_ok() {
                let status = rx.borrow_and_update().clone();
                let _ = app.emit(EVENT_NET_STATUS, status);
            }
        });

        // Scans programmés.
        let app = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(60));
            loop {
                tick.tick().await;
                if let Err(e) = maybe_scheduled_scan(&app).await {
                    tracing::warn!(error = %e, "planificateur de scans");
                }
            }
        });
    }
}

async fn maybe_scheduled_scan(app: &AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    let s = crate::settings::load(&state.db).await?;
    if s.scan_interval_hours == 0 || s.home_world_id.is_none() || state.scans.is_running() {
        return Ok(());
    }
    let last: Option<i64> = sqlx::query_scalar("SELECT MAX(started_at) FROM scan_runs")
        .fetch_one(&state.db)
        .await?;
    let due =
        last.is_none_or(|t| net::now_ms() - t >= i64::from(s.scan_interval_hours) * 3_600_000);
    if due {
        tracing::info!(user_id = state.user_id, "scan programmé");
        scan::start(app)?;
    }
    Ok(())
}

fn spawn_shared_tasks(shared: Arc<Shared>, shared_db: SqlitePool) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(6 * 3600));
        loop {
            tick.tick().await;
            match net::cache_prune(&shared_db).await {
                Ok(n) if n > 0 => tracing::info!(rows = n, "cache API purgé"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "purge du cache impossible"),
            }
            let _ = &shared;
        }
    });
}

/// Équivalent du plugin de notifications : push vers les appareils du compte
/// et événement pour les clients ouverts.
pub struct Notifier(AppHandle);

impl Notifier {
    pub fn builder(&self) -> NotificationBuilder {
        NotificationBuilder {
            app: self.0.clone(),
            title: String::new(),
            body: String::new(),
        }
    }
}

pub struct NotificationBuilder {
    app: AppHandle,
    title: String,
    body: String,
}

#[derive(Serialize, Clone)]
pub struct NotificationPayload {
    pub title: String,
    pub body: String,
}

impl NotificationBuilder {
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
        self
    }

    pub fn show(self) -> Result<(), String> {
        let payload = NotificationPayload {
            title: self.title,
            body: self.body,
        };
        let _ = self.app.emit("notification", payload.clone());
        let app = self.app;
        tokio::spawn(async move {
            let shared = app.shared().clone();
            shared
                .push
                .send(&shared.accounts, app.user_id(), &payload)
                .await;
        });
        Ok(())
    }
}
