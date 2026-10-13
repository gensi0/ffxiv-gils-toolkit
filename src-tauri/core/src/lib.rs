//! Cœur de FFXIV Spender, indépendant de Tauri pour être testable partout.
//!
//! - [`dataset`] : construit, depuis XIVAPI (en français), la liste des échanges
//!   « monnaie → objet revendable à l'hôtel des ventes », avec marchands et
//!   conditions de déblocage.
//! - [`universalis`] : prix et vitesse de vente par serveur.
//! - [`pricing`] : rentabilité en gils par unité de monnaie.

pub mod dataset;
pub mod error;
pub mod http;
pub mod pricing;
pub mod universalis;
pub mod xivapi;

pub use error::{Error, Result};
