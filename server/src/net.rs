//! Shared HTTP plumbing: Universalis throttling (20 req/s, ≤ 8 in flight,
//! exponential backoff on 429/5xx), a TTL response cache in SQLite, and a
//! status channel the UI shows in the status bar.

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use reqwest::{StatusCode, Url};
use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::{watch, Semaphore};

use crate::error::{AppError, AppResult};

pub const MAX_REQUESTS_PER_SECOND: u32 = 20;
/// Universalis allows 8 simultaneous connections per IP.
pub const MAX_IN_FLIGHT: usize = 8;
const MAX_ATTEMPTS: u32 = 6;
const MAX_BACKOFF_SECS: u64 = 60;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetStatus {
    pub in_flight: usize,
    /// Set while waiting after a 429/5xx; unix ms.
    pub backoff_until_ms: Option<i64>,
    pub total_requests: u64,
}

/// Throttled client for Universalis. Cheap to clone.
#[derive(Clone)]
pub struct Throttled {
    http: reqwest::Client,
    limiter: Arc<DefaultDirectRateLimiter>,
    slots: Arc<Semaphore>,
    in_flight: Arc<AtomicUsize>,
    total: Arc<AtomicU64>,
    status: watch::Sender<NetStatus>,
}

impl Throttled {
    pub fn new(http: reqwest::Client) -> Self {
        let rate = NonZeroU32::new(MAX_REQUESTS_PER_SECOND).expect("non-zero");
        Self {
            http,
            limiter: Arc::new(RateLimiter::direct(Quota::per_second(rate))),
            slots: Arc::new(Semaphore::new(MAX_IN_FLIGHT)),
            in_flight: Arc::new(AtomicUsize::new(0)),
            total: Arc::new(AtomicU64::new(0)),
            status: watch::channel(NetStatus::default()).0,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<NetStatus> {
        self.status.subscribe()
    }

    fn publish(&self, backoff_until_ms: Option<i64>) {
        let next = NetStatus {
            in_flight: self.in_flight.load(Ordering::Relaxed),
            backoff_until_ms,
            total_requests: self.total.load(Ordering::Relaxed),
        };
        self.status.send_if_modified(|cur| {
            let changed = *cur != next;
            *cur = next;
            changed
        });
    }

    /// GET `url` and return the body, retrying 429/5xx and transport errors
    /// with exponential backoff (1s, 2s, 4s, … capped at 60s).
    pub async fn get_text(&self, url: &Url) -> AppResult<String> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let outcome = {
                let _slot = self.slots.acquire().await.expect("semaphore never closed");
                self.limiter.until_ready().await;
                self.in_flight.fetch_add(1, Ordering::Relaxed);
                self.total.fetch_add(1, Ordering::Relaxed);
                self.publish(None);
                let res = self
                    .http
                    .get(url.clone())
                    .timeout(REQUEST_TIMEOUT)
                    .send()
                    .await;
                let outcome = match res {
                    Ok(r)
                        if r.status() == StatusCode::TOO_MANY_REQUESTS
                            || r.status().is_server_error() =>
                    {
                        Err(Retry::Status(r.status()))
                    }
                    Ok(r) if r.status().is_success() => r.text().await.map_err(Retry::Transport),
                    Ok(r) => {
                        // Other 4xx: retrying will not help.
                        self.in_flight.fetch_sub(1, Ordering::Relaxed);
                        self.publish(None);
                        return Err(AppError::Other(format!("HTTP {} for {url}", r.status())));
                    }
                    Err(e) => Err(Retry::Transport(e)),
                };
                self.in_flight.fetch_sub(1, Ordering::Relaxed);
                outcome
            };
            match outcome {
                Ok(body) => {
                    self.publish(None);
                    return Ok(body);
                }
                Err(err) if attempt < MAX_ATTEMPTS => {
                    let delay = Duration::from_secs((1u64 << (attempt - 1)).min(MAX_BACKOFF_SECS));
                    tracing::warn!(%url, attempt, ?delay, error = %err, "request failed, backing off");
                    self.publish(Some(now_ms() + delay.as_millis() as i64));
                    tokio::time::sleep(delay).await;
                }
                Err(err) => {
                    self.publish(None);
                    return Err(err.into());
                }
            }
        }
    }
}

#[derive(Debug)]
enum Retry {
    Status(StatusCode),
    Transport(reqwest::Error),
}

impl std::fmt::Display for Retry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Retry::Status(s) => write!(f, "HTTP {s}"),
            Retry::Transport(e) => write!(f, "{e}"),
        }
    }
}

impl From<Retry> for AppError {
    fn from(r: Retry) -> Self {
        match r {
            Retry::Status(s) if s == StatusCode::TOO_MANY_REQUESTS => AppError::Other(
                "Universalis limite le nombre de requêtes (HTTP 429) : réessaie dans une minute"
                    .into(),
            ),
            Retry::Status(s) => AppError::Other(format!("Universalis indisponible (HTTP {s})")),
            Retry::Transport(e) => AppError::Http(e),
        }
    }
}

/// How a cached response may be used.
#[derive(Debug, Clone, Copy)]
pub enum CachePolicy {
    /// Serve from cache if younger than this; otherwise fetch and store.
    MaxAge(Duration),
    /// Always fetch, then store (used by the active scan phase and "Refresh").
    Refresh,
}

pub async fn cache_get(
    pool: &SqlitePool,
    url: &str,
    max_age: Duration,
) -> AppResult<Option<String>> {
    let min_fetched = now_ms() - max_age.as_millis() as i64;
    Ok(
        sqlx::query_scalar("SELECT body FROM api_cache WHERE url = ? AND fetched_at >= ?")
            .bind(url)
            .bind(min_fetched)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn cache_put(pool: &SqlitePool, url: &str, body: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO api_cache (url, body, fetched_at) VALUES (?, ?, ?)
         ON CONFLICT(url) DO UPDATE SET body = excluded.body, fetched_at = excluded.fetched_at",
    )
    .bind(url)
    .bind(body)
    .bind(now_ms())
    .execute(pool)
    .await?;
    Ok(())
}

/// Drops cache rows older than a week (the longest TTL we use).
pub async fn cache_prune(pool: &SqlitePool) -> AppResult<u64> {
    let cutoff = now_ms() - 8 * 24 * 3600 * 1000;
    Ok(sqlx::query("DELETE FROM api_cache WHERE fetched_at < ?")
        .bind(cutoff)
        .execute(pool)
        .await?
        .rows_affected())
}
