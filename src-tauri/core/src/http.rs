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

    /// Corps texte (pages HTML), avec les mêmes réessais.
    pub async fn get_text(&self, url: &str) -> Result<String> {
        let mut delay = Duration::from_millis(500);
        for attempt in 0..3 {
            match self.client.get(url).send().await {
                Ok(resp) if resp.status().is_success() => return Ok(resp.text().await?),
                Ok(resp) if resp.status().as_u16() == 404 => {
                    return Err(Error::Api("page introuvable (404)".into()))
                }
                Ok(resp) if attempt == 2 => {
                    return Err(Error::Api(format!("{} sur {url}", resp.status())))
                }
                Err(e) if attempt == 2 => return Err(e.into()),
                _ => {}
            }
            tokio::time::sleep(delay).await;
            delay *= 2;
        }
        unreachable!("la boucle retourne toujours")
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
