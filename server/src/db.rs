use std::path::{Path, PathBuf};
use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};

use crate::error::AppResult;

/// Opens (creating if needed) the SQLite database and applies migrations.
pub async fn open(path: &Path) -> AppResult<SqlitePool> {
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/// Base des comptes (utilisateurs, sessions, abonnements push).
pub async fn open_accounts(path: &Path) -> AppResult<SqlitePool> {
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./migrations_accounts").run(&pool).await?;
    Ok(pool)
}

/// Copie cohérente de toutes les bases (`VACUUM INTO`), utilisable pendant
/// que le serveur tourne. Renvoie le nombre de bases copiées.
pub async fn backup(data_dir: &Path, dest: &Path) -> AppResult<usize> {
    let io = |e: std::io::Error| crate::error::AppError::Other(format!("sauvegarde : {e}"));
    std::fs::create_dir_all(dest.join("users")).map_err(io)?;
    let mut files = vec![PathBuf::from("accounts.db"), PathBuf::from("shared.db")];
    if let Ok(entries) = std::fs::read_dir(data_dir.join("users")) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".db") {
                files.push(Path::new("users").join(name));
            }
        }
    }
    let mut n = 0;
    for rel in files {
        let src = data_dir.join(&rel);
        if !src.exists() {
            continue;
        }
        let out = dest.join(&rel);
        let _ = std::fs::remove_file(&out);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(SqliteConnectOptions::from_str(&format!(
                "sqlite://{}",
                src.display()
            ))?)
            .await?;
        sqlx::query("VACUUM INTO ?")
            .bind(out.display().to_string())
            .execute(&pool)
            .await?;
        pool.close().await;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
pub async fn open_memory() -> SqlitePool {
    // A single connection: every connection to `:memory:` is a separate database.
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:").unwrap())
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}
