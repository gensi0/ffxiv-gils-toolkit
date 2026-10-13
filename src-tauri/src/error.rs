use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("erreur de base de données : {0}")]
    Db(#[from] sqlx::Error),
    #[error("erreur de migration : {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("erreur réseau : {0}")]
    Http(#[from] reqwest::Error),
    #[error("données inattendues : {0}")]
    Data(#[from] serde_json::Error),
    #[error("saisie invalide : {0}")]
    Invalid(String),
    #[error("{0}")]
    Other(String),
}

// Tauri commands send errors to the frontend as plain strings.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
