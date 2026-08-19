use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Connection {
    pub id: String,
    pub name: String,
    /// "postgres" | "sqlite"
    pub engine: String,
    pub host: Option<String>,
    pub port: Option<i64>,
    /// pg database name, or sqlite file path
    pub database: String,
    pub username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SavedQuery {
    pub id: String,
    pub connection_id: String,
    pub name: String,
    pub sql: String,
}

pub async fn init(db_path: &Path) -> Result<SqlitePool, sqlx::Error> {
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePool::connect_with(opts).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS connections (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            engine TEXT NOT NULL CHECK(engine IN ('postgres','sqlite')),
            host TEXT,
            port INTEGER,
            database TEXT NOT NULL,
            username TEXT
        )",
    )
    .execute(&pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS saved_queries (
            id TEXT PRIMARY KEY,
            connection_id TEXT NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            sql TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await?;
    Ok(pool)
}

const KEYRING_SERVICE: &str = "dbelte";

pub fn set_password(conn_id: &str, password: &str) -> Result<(), String> {
    keyring::Entry::new(KEYRING_SERVICE, conn_id)
        .and_then(|e| e.set_password(password))
        .map_err(|e| e.to_string())
}

pub fn get_password(conn_id: &str) -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, conn_id)
        .and_then(|e| e.get_password())
        .ok()
}

pub fn delete_password(conn_id: &str) {
    if let Ok(e) = keyring::Entry::new(KEYRING_SERVICE, conn_id) {
        let _ = e.delete_credential();
    }
}
