//! Comptes : inscription avec code d'invitation, connexion, session par cookie.

use std::sync::Arc;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine as _;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::app::AppHandle;
use crate::net::now_ms;
use crate::Server;

const COOKIE: &str = "gils_session";
/// Une session dure 90 jours, prolongée à chaque usage.
const SESSION_MS: i64 = 90 * 24 * 3600 * 1000;

/// Erreur renvoyée au client : `{ "error": "…" }` avec un statut HTTP.
pub struct ApiError(pub StatusCode, pub String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

impl<E: std::fmt::Display> From<E> for ApiError {
    fn from(e: E) -> Self {
        tracing::warn!(error = %e, "erreur interne");
        Self(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
}

fn bad(msg: &str) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.into())
}

fn hash_token(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn cookie_value(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|c| c.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE)
        .map(|(_, v)| v.to_string())
}

fn session_cookie(server: &Server, token: &str, max_age_s: i64) -> HeaderValue {
    let secure = if server.cookie_secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_s}{secure}"
    ))
    .expect("cookie ASCII")
}

/// Compte connecté, extrait du cookie de session.
pub struct User(pub AppHandle);

impl FromRequestParts<Arc<Server>> for User {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, server: &Arc<Server>) -> Result<Self, ApiError> {
        let unauthorized = || ApiError(StatusCode::UNAUTHORIZED, "non connecté".into());
        let token = cookie_value(&parts.headers).ok_or_else(unauthorized)?;
        let row: Option<(i64, String, i64)> = sqlx::query_as(
            "SELECT u.id, u.username, s.expires_at FROM sessions s
             JOIN users u ON u.id = s.user_id WHERE s.token_hash = ?",
        )
        .bind(hash_token(&token))
        .fetch_optional(&server.shared.accounts)
        .await?;
        let Some((user_id, username, expires_at)) = row else {
            return Err(unauthorized());
        };
        let now = now_ms();
        if expires_at < now {
            return Err(unauthorized());
        }
        // Prolonge la session au plus une fois par jour.
        if expires_at - now < SESSION_MS - 24 * 3600 * 1000 {
            sqlx::query("UPDATE sessions SET expires_at = ? WHERE token_hash = ?")
                .bind(now + SESSION_MS)
                .bind(hash_token(&token))
                .execute(&server.shared.accounts)
                .await?;
        }
        Ok(User(server.shared.user(user_id, &username).await?))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
    username: String,
    password: String,
    #[serde(default)]
    invite: Option<String>,
}

#[derive(Serialize)]
pub struct Me {
    username: String,
}

fn valid_username(name: &str) -> bool {
    (3..=32).contains(&name.chars().count())
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '))
}

async fn open_session(server: &Server, user_id: i64) -> Result<HeaderValue, ApiError> {
    let mut raw = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut raw);
    let token = B64.encode(raw);
    let now = now_ms();
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(hash_token(&token))
    .bind(user_id)
    .bind(now)
    .bind(now + SESSION_MS)
    .execute(&server.shared.accounts)
    .await?;
    Ok(session_cookie(server, &token, SESSION_MS / 1000))
}

pub async fn register(
    State(server): State<Arc<Server>>,
    Json(c): Json<Credentials>,
) -> Result<Response, ApiError> {
    let Some(expected) = server.invite_code.as_deref() else {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "inscriptions fermées (INVITE_CODE non défini sur le serveur)".into(),
        ));
    };
    if c.invite.as_deref().map(str::trim) != Some(expected) {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "code d'invitation incorrect".into(),
        ));
    }
    let username = c.username.trim().to_string();
    if !valid_username(&username) {
        return Err(bad(
            "identifiant : 3 à 32 caractères (lettres, chiffres, espace, _ - .)",
        ));
    }
    if c.password.chars().count() < 8 {
        return Err(bad("mot de passe : 8 caractères minimum"));
    }
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(c.password.as_bytes(), &salt)
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .to_string();
    let inserted =
        sqlx::query("INSERT INTO users (username, password_hash, created_at) VALUES (?, ?, ?)")
            .bind(&username)
            .bind(&hash)
            .bind(now_ms())
            .execute(&server.shared.accounts)
            .await;
    let user_id = match inserted {
        Ok(r) => r.last_insert_rowid(),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "cet identifiant est déjà pris".into(),
            ));
        }
        Err(e) => return Err(e.into()),
    };
    tracing::info!(user_id, "compte créé");
    let cookie = open_session(&server, user_id).await?;
    Ok(([(header::SET_COOKIE, cookie)], Json(Me { username })).into_response())
}

pub async fn login(
    State(server): State<Arc<Server>>,
    Json(c): Json<Credentials>,
) -> Result<Response, ApiError> {
    let row: Option<(i64, String, String)> =
        sqlx::query_as("SELECT id, username, password_hash FROM users WHERE username = ?")
            .bind(c.username.trim())
            .fetch_optional(&server.shared.accounts)
            .await?;
    let wrong = || {
        ApiError(
            StatusCode::UNAUTHORIZED,
            "identifiant ou mot de passe incorrect".into(),
        )
    };
    let Some((user_id, username, hash)) = row else {
        // Même coût qu'une vraie vérification : ne révèle pas si le compte existe.
        let _ = Argon2::default()
            .hash_password(c.password.as_bytes(), &SaltString::generate(&mut OsRng));
        return Err(wrong());
    };
    let parsed = PasswordHash::new(&hash)
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if Argon2::default()
        .verify_password(c.password.as_bytes(), &parsed)
        .is_err()
    {
        return Err(wrong());
    }
    let cookie = open_session(&server, user_id).await?;
    Ok(([(header::SET_COOKIE, cookie)], Json(Me { username })).into_response())
}

pub async fn logout(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(token) = cookie_value(&headers) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(hash_token(&token))
            .execute(&server.shared.accounts)
            .await?;
    }
    Ok((
        [(header::SET_COOKIE, session_cookie(&server, "", 0))],
        StatusCode::NO_CONTENT,
    )
        .into_response())
}

pub async fn me(User(app): User) -> Json<Me> {
    Json(Me {
        username: app.username().to_string(),
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordChange {
    current: String,
    new: String,
}

pub async fn change_password(
    State(server): State<Arc<Server>>,
    User(app): User,
    Json(p): Json<PasswordChange>,
) -> Result<StatusCode, ApiError> {
    if p.new.chars().count() < 8 {
        return Err(bad("mot de passe : 8 caractères minimum"));
    }
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
        .bind(app.user_id())
        .fetch_one(&server.shared.accounts)
        .await?;
    let parsed = PasswordHash::new(&hash)
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if Argon2::default()
        .verify_password(p.current.as_bytes(), &parsed)
        .is_err()
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "mot de passe actuel incorrect".into(),
        ));
    }
    let new = Argon2::default()
        .hash_password(p.new.as_bytes(), &SaltString::generate(&mut OsRng))
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .to_string();
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(new)
        .bind(app.user_id())
        .execute(&server.shared.accounts)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames() {
        assert!(valid_username("Gen Si"));
        assert!(valid_username("théo_2"));
        assert!(!valid_username("ab"));
        assert!(!valid_username("a<script>"));
    }

    #[test]
    fn cookie_is_found_among_others() {
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            HeaderValue::from_static("a=1; gils_session=abc; b=2"),
        );
        assert_eq!(cookie_value(&h).as_deref(), Some("abc"));
    }
}
