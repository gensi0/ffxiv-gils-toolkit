//! Lecture des feuilles du jeu via XIVAPI v2, toujours en français.
//!
//! Les références vers d'autres feuilles sont demandées en `@as(raw)` : sans
//! cela XIVAPI résout chaque lien et refuse les requêtes qui dépassent
//! 20 000 lignes traitées (une page de boutiques en contient bien plus).

use futures::stream::{self, StreamExt, TryStreamExt};
use serde::Deserialize;
use serde_json::Value;

use crate::http::Http;
use crate::Result;

const BASE: &str = "https://v2.xivapi.com/api";
const PAGE: usize = 500;
/// XIVAPI plafonne `rows=` à 100 identifiants par requête.
const ROWS_PER_REQUEST: usize = 100;

#[derive(Debug, Clone, Deserialize)]
pub struct Row {
    pub row_id: u32,
    #[serde(default)]
    pub subrow_id: Option<u32>,
    pub fields: Value,
}

#[derive(Deserialize)]
struct Page {
    rows: Vec<Row>,
}

#[derive(Clone)]
pub struct XivApi {
    http: Http,
}

fn url(path: &str, query: &[(&str, &str)]) -> String {
    let mut u = format!("{BASE}/{path}?language=fr");
    for (k, v) in query {
        u.push('&');
        u.push_str(k);
        u.push('=');
        u.push_str(&encode(v));
    }
    u
}

/// Encodage minimal des paramètres : virgules, crochets, @ et parenthèses sont
/// acceptés tels quels par XIVAPI, pas les opérateurs de recherche.
fn encode(s: &str) -> String {
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('&', "%26")
        .replace('+', "%2B")
        .replace('=', "%3D")
        .replace('<', "%3C")
        .replace('>', "%3E")
}

impl XivApi {
    pub fn new(http: Http) -> Self {
        Self { http }
    }

    /// Toutes les lignes d'une feuille, page par page.
    pub async fn sheet(&self, sheet: &str, fields: &str) -> Result<Vec<Row>> {
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let limit = PAGE.to_string();
            let mut q = vec![("limit", limit.as_str()), ("fields", fields)];
            if let Some(a) = &after {
                q.push(("after", a.as_str()));
            }
            let page: Page = self
                .http
                .get_json(&url(&format!("sheet/{sheet}"), &q))
                .await?;
            let n = page.rows.len();
            if let Some(last) = page.rows.last() {
                after = Some(match last.subrow_id {
                    Some(sub) => format!("{}:{sub}", last.row_id),
                    None => last.row_id.to_string(),
                });
            }
            out.extend(page.rows);
            if n < PAGE {
                return Ok(out);
            }
        }
    }

    /// Lignes précises d'une feuille (par lots de 100, 4 lots en parallèle).
    pub async fn rows(&self, sheet: &str, ids: &[u32], fields: &str) -> Result<Vec<Row>> {
        let chunks: Vec<String> = ids
            .chunks(ROWS_PER_REQUEST)
            .map(|c| c.iter().map(u32::to_string).collect::<Vec<_>>().join(","))
            .collect();
        let pages: Vec<Page> = stream::iter(chunks)
            .map(|ids| {
                let u = url(
                    &format!("sheet/{sheet}"),
                    &[("rows", &ids), ("fields", fields)],
                );
                async move { self.http.get_json::<Page>(&u).await }
            })
            .buffer_unordered(4)
            .try_collect()
            .await?;
        Ok(pages.into_iter().flat_map(|p| p.rows).collect())
    }
}

#[derive(Deserialize)]
struct SearchPage {
    results: Vec<Row>,
    next: Option<String>,
}

impl XivApi {
    /// Recherche paginée (`query` au format XIVAPI, ex. `+Rarity>=2`).
    pub async fn search(&self, sheet: &str, query: &str, fields: &str) -> Result<Vec<Row>> {
        let limit = PAGE.to_string();
        let mut out = Vec::new();
        let mut page: SearchPage = self
            .http
            .get_json(&url(
                "search",
                &[
                    ("sheets", sheet),
                    ("query", query),
                    ("fields", fields),
                    ("limit", &limit),
                ],
            ))
            .await?;
        loop {
            let done = page.results.is_empty();
            out.extend(page.results);
            match page.next {
                Some(cursor) if !done => {
                    page = self
                        .http
                        .get_json(&url(
                            "search",
                            &[("cursor", &cursor), ("fields", fields), ("limit", &limit)],
                        ))
                        .await?;
                }
                _ => return Ok(out),
            }
        }
    }
}

/// Petits accesseurs tolérants sur les champs JSON.
pub trait FieldsExt {
    fn u32_of(&self, key: &str) -> u32;
    fn str_of(&self, key: &str) -> String;
    fn u32s_of(&self, key: &str) -> Vec<u32>;
}

impl FieldsExt for Value {
    fn u32_of(&self, key: &str) -> u32 {
        self.get(key).and_then(Value::as_u64).unwrap_or(0) as u32
    }
    fn str_of(&self, key: &str) -> String {
        self.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }
    fn u32s_of(&self, key: &str) -> Vec<u32> {
        self.get(key)
            .and_then(Value::as_array)
            .map(|a| a.iter().map(|v| v.as_u64().unwrap_or(0) as u32).collect())
            .unwrap_or_default()
    }
}

/// Chemin d'icône du jeu (`ui/icon/065000/065070.tex`), si présent.
pub fn icon_path(fields: &Value) -> Option<String> {
    fields
        .get("Icon")
        .and_then(|i| i.get("path"))
        .and_then(Value::as_str)
        .filter(|p| !p.ends_with("000000.tex"))
        .map(str::to_string)
}
