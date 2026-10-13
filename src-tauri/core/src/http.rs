//! Client HTTP partagé : User-Agent explicite (XIVAPI refuse les clients
//! anonymes), réessais sur 429 / 5xx avec attente croissante.

use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::{Error, Result};

#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
}

impl Default for Http {
    fn default() -> Self {
        Self::new()
    }
}

impl Http {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!("ffxiv-spender/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(60))
            .build()
            .expect("client HTTP");
        Self { client }
    }

    pub async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let mut delay = Duration::from_millis(500);
        for attempt in 0..5 {
            match self.client.get(url).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        return Ok(resp.json::<T>().await?);
                    }
                    let retry = status.as_u16() == 429 || status.is_server_error();
                    if !retry || attempt == 4 {
                        let body = resp.text().await.unwrap_or_default();
                        return Err(Error::Api(format!(
                            "{status} sur {url} : {}",
                            body.chars().take(300).collect::<String>()
                        )));
                    }
                }
                Err(e) if attempt == 4 => return Err(e.into()),
                Err(e) => tracing::warn!(error = %e, url, "requête échouée, nouvel essai"),
            }
            tokio::time::sleep(delay).await;
            delay *= 2;
        }
        unreachable!("la boucle retourne toujours")
    }
}
