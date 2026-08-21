use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use sqlx::{Connection as _, SqliteConnection};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Connection {
    pub id: String,
    pub name: String,
    /// the engine key `db::open` dispatches on
    pub engine: String,
    pub host: Option<String>,
    pub port: Option<i64>,
    /// database name for a server engine, or the file path for sqlite
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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct HistoryEntry {
    pub id: i64,
    pub sql: String,
    /// UTC, "YYYY-MM-DD HH:MM:SS" as SQLite's datetime() writes it
    pub ran_at: String,
}

/// How many statements to keep per connection. Enough to find yesterday's
/// query, small enough that nobody has to think about the file growing.
const HISTORY_LIMIT: i64 = 50;

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
            engine TEXT NOT NULL,
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
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS query_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            connection_id TEXT NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
            sql TEXT NOT NULL,
            ran_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(&pool)
    .await?;
    drop_engine_check(&pool).await?;
    Ok(pool)
}

/// `connections.engine` used to carry `CHECK(engine IN ('postgres','sqlite'))`.
/// That constraint lives inside the CREATE statement, which SQLite cannot ALTER
/// and `CREATE TABLE IF NOT EXISTS` will not revisit, so an install predating a
/// new engine has to have the table rebuilt. Which engine keys are valid is
/// `db::open`'s call, not the storage layer's, so the check goes rather than
/// grows — this is the last migration engine names will need.
async fn drop_engine_check(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let ddl: Option<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'connections'",
    )
    .fetch_optional(pool)
    .await?;
    if !ddl.unwrap_or_default().contains("CHECK(engine IN") {
        return Ok(());
    }
    let mut c = pool.acquire().await?;
    // saved_queries and query_history reference connections(id); dropping the
    // table with enforcement on would take their rows with it
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *c)
        .await?;
    let rebuilt = rebuild_connections(&mut c).await;
    // restore before propagating: this connection goes back into the pool
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *c)
        .await?;
    rebuilt
}

async fn rebuild_connections(c: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    let mut tx = c.begin().await?;
    for stmt in [
        "CREATE TABLE connections_new (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            engine TEXT NOT NULL,
            host TEXT,
            port INTEGER,
            database TEXT NOT NULL,
            username TEXT
        )",
        "INSERT INTO connections_new
             SELECT id, name, engine, host, port, database, username FROM connections",
        "DROP TABLE connections",
        "ALTER TABLE connections_new RENAME TO connections",
    ] {
        sqlx::query(stmt).execute(&mut *tx).await?;
    }
    tx.commit().await
}

/// Remember a statement that ran. Re-running the same SQL doesn't stack up
/// duplicates, and only the last `HISTORY_LIMIT` per connection are kept.
pub async fn record_history(
    pool: &SqlitePool,
    connection_id: &str,
    sql: &str,
) -> Result<(), sqlx::Error> {
    let last: Option<String> = sqlx::query_scalar(
        "SELECT sql FROM query_history WHERE connection_id = ? ORDER BY id DESC LIMIT 1",
    )
    .bind(connection_id)
    .fetch_optional(pool)
    .await?;
    if last.as_deref() == Some(sql) {
        return Ok(());
    }
    sqlx::query("INSERT INTO query_history (connection_id, sql) VALUES (?, ?)")
        .bind(connection_id)
        .bind(sql)
        .execute(pool)
        .await?;
    sqlx::query(
        "DELETE FROM query_history
         WHERE connection_id = ?1
           AND id NOT IN (
               SELECT id FROM query_history WHERE connection_id = ?1
               ORDER BY id DESC LIMIT ?2
           )",
    )
    .bind(connection_id)
    .bind(HISTORY_LIMIT)
    .execute(pool)
    .await?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn history_dedupes_and_trims() {
        let path = std::env::temp_dir().join(format!("dbelte-history-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let pool = init(&path).await.expect("init meta db");
        sqlx::query("INSERT INTO connections (id, name, engine, database) VALUES ('c', 'c', 'sqlite', ':memory:')")
            .execute(&pool)
            .await
            .unwrap();

        record_history(&pool, "c", "SELECT 1").await.unwrap();
        record_history(&pool, "c", "SELECT 1").await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM query_history")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 1, "re-running the same statement shouldn't stack up");

        for i in 0..HISTORY_LIMIT + 10 {
            record_history(&pool, "c", &format!("SELECT {i}")).await.unwrap();
        }
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM query_history")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, HISTORY_LIMIT);
        let newest: String = sqlx::query_scalar("SELECT sql FROM query_history ORDER BY id DESC LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(newest, format!("SELECT {}", HISTORY_LIMIT + 9));

        drop(pool);
        let _ = std::fs::remove_file(&path);
    }

    /// An install created before a new engine was added carries the old CHECK.
    /// Rebuilding must widen it without losing connections or their children.
    #[tokio::test]
    async fn migrates_away_the_old_engine_check() {
        let path = std::env::temp_dir().join(format!("dbelte-migrate-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);

        // an old-format meta.db, CHECK and all
        let pool = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", path.display()))
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE connections (
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
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO connections (id, name, engine, database) VALUES ('c', 'old', 'sqlite', 'x.db')",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            sqlx::query("INSERT INTO connections (id, name, engine, database) VALUES ('m', 'm', 'mysql', 'd')")
                .execute(&pool)
                .await
                .is_err(),
            "the old CHECK should still be rejecting mysql before we migrate"
        );
        drop(pool);

        let pool = init(&path).await.expect("init over an old meta db");
        // the existing connection and its history survived the rebuild
        sqlx::query("INSERT INTO saved_queries (id, connection_id, name, sql) VALUES ('q', 'c', 'q', 'SELECT 1')")
            .execute(&pool)
            .await
            .expect("the foreign key still points at a live connections row");
        let name: String = sqlx::query_scalar("SELECT name FROM connections WHERE id = 'c'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(name, "old");
        // and a new engine now fits
        sqlx::query("INSERT INTO connections (id, name, engine, database) VALUES ('m', 'm', 'mysql', 'd')")
            .execute(&pool)
            .await
            .unwrap();

        drop(pool);
        let _ = std::fs::remove_file(&path);
    }
}
