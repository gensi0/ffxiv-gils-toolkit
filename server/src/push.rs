//! Notifications push (Web Push, RFC 8030) vers les appareils d'un compte.
//! La clé VAPID est créée au premier démarrage et gardée dans la base des
//! comptes : les abonnements restent valides d'un redémarrage à l'autre.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine as _;
use serde::Deserialize;
use sqlx::SqlitePool;
use web_push_native::jwt_simple::algorithms::ES256KeyPair;
use web_push_native::p256::PublicKey;
use web_push_native::{Auth, WebPushBuilder};

use crate::app::NotificationPayload;
use crate::error::{AppError, AppResult};
use crate::net::now_ms;

/// Contact exigé par les services push (VAPID).
const CONTACT: &str = "mailto:admin@gensi.eu";

pub struct Push {
    key: ES256KeyPair,
    http: reqwest::Client,
}

/// Abonnement envoyé par le navigateur (`PushSubscription.toJSON()`).
#[derive(Debug, Clone, Deserialize)]
pub struct Subscription {
    pub endpoint: String,
    pub keys: SubscriptionKeys,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubscriptionKeys {
    pub p256dh: String,
    pub auth: String,
}

impl Push {
    pub async fn load(accounts: &SqlitePool, http: reqwest::Client) -> AppResult<Self> {
        let stored: Option<String> =
            sqlx::query_scalar("SELECT value FROM server_meta WHERE key = 'vapid_private'")
                .fetch_optional(accounts)
                .await?;
        let key = match stored.and_then(|s| B64.decode(s).ok()) {
            Some(bytes) => ES256KeyPair::from_bytes(&bytes)
                .map_err(|e| AppError::Other(format!("clé VAPID illisible : {e}")))?,
            None => {
                let key = ES256KeyPair::generate();
                sqlx::query(
                    "INSERT OR REPLACE INTO server_meta (key, value) VALUES ('vapid_private', ?)",
                )
                .bind(B64.encode(key.to_bytes()))
                .execute(accounts)
                .await?;
                tracing::info!("clé VAPID créée");
                key
            }
        };
        Ok(Self { key, http })
    }

    /// Clé publique à donner au navigateur (`applicationServerKey`).
    pub fn public_key(&self) -> String {
        use web_push_native::p256::elliptic_curve::sec1::ToEncodedPoint as _;
        let secret = web_push_native::p256::SecretKey::from_slice(&self.key.to_bytes())
            .expect("clé VAPID valide");
        B64.encode(secret.public_key().to_encoded_point(false).as_bytes())
    }

    pub async fn subscribe(accounts: &SqlitePool, user_id: i64, s: &Subscription) -> AppResult<()> {
        // Vérifie les clés avant de les enregistrer.
        builder(s).map_err(AppError::Invalid)?;
        sqlx::query(
            "INSERT INTO push_subscriptions (endpoint, user_id, p256dh, auth, created_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(endpoint) DO UPDATE SET
               user_id = excluded.user_id, p256dh = excluded.p256dh, auth = excluded.auth",
        )
        .bind(&s.endpoint)
        .bind(user_id)
        .bind(&s.keys.p256dh)
        .bind(&s.keys.auth)
        .bind(now_ms())
        .execute(accounts)
        .await?;
        Ok(())
    }

    pub async fn unsubscribe(accounts: &SqlitePool, user_id: i64, endpoint: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM push_subscriptions WHERE endpoint = ? AND user_id = ?")
            .bind(endpoint)
            .bind(user_id)
            .execute(accounts)
            .await?;
        Ok(())
    }

    /// Envoie à tous les appareils du compte ; les abonnements expirés (404,
    /// 410) sont supprimés.
    pub async fn send(&self, accounts: &SqlitePool, user_id: i64, n: &NotificationPayload) {
        let subs: Vec<(String, String, String)> = match sqlx::query_as(
            "SELECT endpoint, p256dh, auth FROM push_subscriptions WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_all(accounts)
        .await
        {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "abonnements push illisibles");
                return;
            }
        };
        let Ok(body) = serde_json::to_vec(n) else {
            return;
        };
        for (endpoint, p256dh, auth) in subs {
            let sub = Subscription {
                endpoint: endpoint.clone(),
                keys: SubscriptionKeys { p256dh, auth },
            };
            match self.deliver(&sub, &body).await {
                Ok(status) if status == 404 || status == 410 => {
                    let _ = Self::unsubscribe(accounts, user_id, &endpoint).await;
                }
                Ok(status) if !(200..300).contains(&status) => {
                    tracing::warn!(status, "push refusé");
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "push impossible"),
            }
        }
    }

    async fn deliver(&self, sub: &Subscription, body: &[u8]) -> Result<u16, String> {
        let req = builder(sub)?
            .with_valid_duration(std::time::Duration::from_secs(86_400))
            .with_vapid(&self.key, CONTACT)
            .build(body.to_vec())
            .map_err(|e| e.to_string())?;
        let (parts, body) = req.into_parts();
        let mut r = self
            .http
            .post(parts.uri.to_string())
            .header("Urgency", "high")
            .body(body);
        for (name, value) in &parts.headers {
            // reqwest calcule lui-même la longueur du corps.
            if name == http::header::CONTENT_LENGTH {
                continue;
            }
            r = r.header(name.as_str(), value.as_bytes());
        }
        let resp = r.send().await.map_err(|e| e.to_string())?;
        Ok(resp.status().as_u16())
    }
}

fn builder(s: &Subscription) -> Result<WebPushBuilder, String> {
    let key = B64
        .decode(s.keys.p256dh.trim_end_matches('='))
        .map_err(|e| format!("clé p256dh : {e}"))?;
    let auth = B64
        .decode(s.keys.auth.trim_end_matches('='))
        .map_err(|e| format!("clé auth : {e}"))?;
    if auth.len() != 16 {
        return Err("clé auth : 16 octets attendus".into());
    }
    Ok(WebPushBuilder::new(
        s.endpoint
            .parse()
            .map_err(|e| format!("adresse push : {e}"))?,
        PublicKey::from_sec1_bytes(&key).map_err(|e| format!("clé p256dh : {e}"))?,
        Auth::clone_from_slice(&auth),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use web_push_native::p256::elliptic_curve::sec1::ToEncodedPoint as _;
    use web_push_native::p256::SecretKey;

    #[test]
    fn message_is_encrypted_for_the_device_and_signed() {
        // Clés d'un « navigateur » fictif.
        let ua_secret = SecretKey::random(&mut argon2::password_hash::rand_core::OsRng);
        let auth = [7u8; 16];
        let sub = Subscription {
            endpoint: "https://fcm.googleapis.com/fcm/send/abc".into(),
            keys: SubscriptionKeys {
                p256dh: B64.encode(ua_secret.public_key().to_encoded_point(false).as_bytes()),
                auth: B64.encode(auth),
            },
        };
        let vapid = ES256KeyPair::generate();
        let req = builder(&sub)
            .unwrap()
            .with_vapid(&vapid, CONTACT)
            .build(b"{\"title\":\"t\",\"body\":\"b\"}".to_vec())
            .unwrap();
        let header = req.headers()["authorization"].to_str().unwrap();
        assert!(header.starts_with("vapid t="), "{header}");
        assert_eq!(req.uri().host(), Some("fcm.googleapis.com"));
        let clear =
            web_push_native::decrypt(req.into_body(), &ua_secret, &Auth::clone_from_slice(&auth))
                .unwrap();
        assert_eq!(clear, b"{\"title\":\"t\",\"body\":\"b\"}");
    }

    #[test]
    fn bad_keys_are_rejected() {
        let sub = Subscription {
            endpoint: "https://example.com/x".into(),
            keys: SubscriptionKeys {
                p256dh: "pas-une-cle".into(),
                auth: "AAAA".into(),
            },
        };
        assert!(builder(&sub).is_err());
    }
}
