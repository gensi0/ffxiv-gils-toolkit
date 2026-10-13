#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("réseau : {0}")]
    Http(#[from] reqwest::Error),
    #[error("réponse illisible : {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Api(String),
    #[error("fichier : {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
