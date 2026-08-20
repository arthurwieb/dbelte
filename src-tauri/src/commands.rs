use crate::db::{self, Bind, ColumnInfo, DbPool, Filter, QueryResult, Sort};
use crate::meta::{self, Connection, SavedQuery};
use serde_json::Value;
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;
use tokio::sync::{oneshot, Mutex};

pub struct AppState {
    pub meta: SqlitePool,
    /// Arc so a handler can take a pool and release the map lock immediately.
    pub pools: Mutex<HashMap<String, Arc<DbPool>>>,
    /// Queries currently in flight, keyed by the id the frontend generated.
    pub running: Mutex<HashMap<String, RunningQuery>>,
}

pub struct RunningQuery {
    conn_id: String,
    /// Postgres backend PID, so another session can pg_cancel_backend it.
    pg_pid: Option<i32>,
    /// Fires to drop the in-flight future, unblocking the tab.
    abort: oneshot::Sender<()>,
}

/// Error text for a cancelled query; the frontend matches on it to stay quiet.
const CANCELLED: &str = "query cancelled";

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// ---------- connections ----------

#[tauri::command]
pub async fn list_connections(state: State<'_, AppState>) -> CmdResult<Vec<Connection>> {
    sqlx::query_as::<_, Connection>("SELECT * FROM connections ORDER BY name")
        .fetch_all(&state.meta)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn save_connection(
    state: State<'_, AppState>,
    mut conn: Connection,
    password: Option<String>,
) -> CmdResult<Connection> {
    if conn.id.is_empty() {
        conn.id = uuid::Uuid::new_v4().to_string();
    }
    sqlx::query(
        "INSERT INTO connections (id, name, engine, host, port, database, username)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           name=excluded.name, engine=excluded.engine, host=excluded.host,
           port=excluded.port, database=excluded.database, username=excluded.username",
    )
    .bind(&conn.id)
    .bind(&conn.name)
    .bind(&conn.engine)
    .bind(&conn.host)
    .bind(conn.port)
    .bind(&conn.database)
    .bind(&conn.username)
    .execute(&state.meta)
    .await
    .map_err(err)?;
    if let Some(pw) = password.filter(|p| !p.is_empty()) {
        meta::set_password(&conn.id, &pw)?;
    }
    Ok(conn)
}

#[tauri::command]
pub async fn delete_connection(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.pools.lock().await.remove(&id);
    sqlx::query("DELETE FROM connections WHERE id = ?")
        .bind(&id)
        .execute(&state.meta)
        .await
        .map_err(err)?;
    meta::delete_password(&id);
    Ok(())
}

#[tauri::command]
pub async fn test_connection(conn: Connection, password: Option<String>) -> CmdResult<()> {
    let password = password.filter(|p| !p.is_empty()).or_else(|| {
        (!conn.id.is_empty())
            .then(|| meta::get_password(&conn.id))
            .flatten()
    });
    let pool = db::open(&conn, password).await?;
    db::execute(&pool, "SELECT 1", vec![]).await?;
    Ok(())
}

#[tauri::command]
pub async fn connect(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let conn = get_connection(&state.meta, &id).await?;
    let password = meta::get_password(&id);
    let pool = db::open(&conn, password).await?;
    state.pools.lock().await.insert(id, Arc::new(pool));
    Ok(())
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.pools.lock().await.remove(&id);
    Ok(())
}

async fn get_connection(meta: &SqlitePool, id: &str) -> CmdResult<Connection> {
    sqlx::query_as::<_, Connection>("SELECT * FROM connections WHERE id = ?")
        .bind(id)
        .fetch_optional(meta)
        .await
        .map_err(err)?
        .ok_or_else(|| "connection not found".into())
}

async fn pool_for(state: &State<'_, AppState>, id: &str) -> CmdResult<Arc<DbPool>> {
    state
        .pools
        .lock()
        .await
        .get(id)
        .cloned()
        .ok_or_else(|| "not connected".to_string())
}

/// Run `f` with the open pool for `id`. Takes a clone of the `Arc` and drops
/// the map lock straight away — holding it for the length of a query would
/// block every other command in the app, cancellation included.
macro_rules! with_pool {
    ($state:expr, $id:expr, $pool:ident => $body:expr) => {{
        let pool = pool_for(&$state, &$id).await?;
        let $pool = &*pool;
        $body
    }};
}

// ---------- introspection ----------

#[tauri::command]
pub async fn list_tables(state: State<'_, AppState>, id: String) -> CmdResult<Vec<String>> {
    with_pool!(state, id, pool => {
        let sql = match pool {
            DbPool::Pg(_) => {
                "SELECT table_name FROM information_schema.tables
                 WHERE table_schema = 'public' AND table_type = 'BASE TABLE' ORDER BY 1"
            }
            DbPool::Sqlite(_) => {
                "SELECT name FROM sqlite_master
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY 1"
            }
        };
        let res = db::execute(pool, sql, vec![]).await?;
        Ok(res
            .rows
            .into_iter()
            .filter_map(|mut r| r.pop().and_then(|v| v.as_str().map(String::from)))
            .collect())
    })
}

async fn fetch_schema(pool: &DbPool, table: &str) -> CmdResult<Vec<ColumnInfo>> {
    match pool {
        DbPool::Pg(p) => {
            // format_type (not information_schema.data_type) so the type name is
            // always castable — enums and arrays come back as USER-DEFINED/ARRAY there
            let cols = sqlx::query(
                "SELECT a.attname,
                        format_type(a.atttypid, a.atttypmod),
                        NOT a.attnotnull,
                        COALESCE(i.indisprimary, false)
                 FROM pg_attribute a
                 LEFT JOIN pg_index i
                   ON i.indrelid = a.attrelid AND a.attnum = ANY(i.indkey) AND i.indisprimary
                 WHERE a.attrelid = ($1::text)::regclass
                   AND a.attnum > 0 AND NOT a.attisdropped
                 ORDER BY a.attnum",
            )
            .bind(db::quote_ident(table))
            .fetch_all(p)
            .await
            .map_err(err)?;
            if cols.is_empty() {
                return Err(format!("unknown table: {table}"));
            }
            Ok(cols
                .iter()
                .map(|r| ColumnInfo {
                    name: r.get(0),
                    data_type: r.get(1),
                    nullable: r.get(2),
                    is_pk: r.get(3),
                })
                .collect())
        }
        DbPool::Sqlite(p) => {
            // PRAGMA can't take bound params: verify the table exists first
            let exists = sqlx::query(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?",
            )
            .bind(table)
            .fetch_optional(p)
            .await
            .map_err(err)?;
            if exists.is_none() {
                return Err(format!("unknown table: {table}"));
            }
            let rows = sqlx::query(&format!(
                "PRAGMA table_info({})",
                db::quote_ident(table)
            ))
            .fetch_all(p)
            .await
            .map_err(err)?;
            Ok(rows
                .iter()
                .map(|r| ColumnInfo {
                    name: r.get::<String, _>("name"),
                    data_type: r.get::<String, _>("type"),
                    nullable: r.get::<i64, _>("notnull") == 0,
                    is_pk: r.get::<i64, _>("pk") > 0,
                })
                .collect())
        }
    }
}

#[tauri::command]
pub async fn table_schema(
    state: State<'_, AppState>,
    id: String,
    table: String,
) -> CmdResult<Vec<ColumnInfo>> {
    with_pool!(state, id, pool => fetch_schema(pool, &table).await)
}

// ---------- data ----------

#[tauri::command]
pub async fn fetch_rows(
    state: State<'_, AppState>,
    id: String,
    table: String,
    filters: Vec<Filter>,
    sort: Option<Sort>,
    limit: i64,
    offset: i64,
) -> CmdResult<QueryResult> {
    with_pool!(state, id, pool => {
        let schema = fetch_schema(pool, &table).await?;
        let (sql, binds) = db::build_select(
            &table,
            &schema,
            &filters,
            sort.as_ref(),
            limit,
            offset,
            matches!(pool, DbPool::Pg(_)),
        )?;
        db::execute(pool, &sql, binds).await
    })
}

#[tauri::command]
pub async fn run_query(
    state: State<'_, AppState>,
    id: String,
    sql: String,
    query_id: String,
) -> CmdResult<QueryResult> {
    let (abort, abort_rx) = oneshot::channel::<()>();
    let pool = pool_for(&state, &id).await?;
    let out = match &*pool {
        DbPool::Pg(p) => {
            // pin the query to one connection so the PID we register belongs to
            // the session actually running it
            let mut conn = p.acquire().await.map_err(err)?;
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                .fetch_one(&mut *conn)
                .await
                .map_err(err)?;
            state.running.lock().await.insert(
                query_id.clone(),
                RunningQuery {
                    conn_id: id.clone(),
                    pg_pid: Some(pid),
                    abort,
                },
            );
            tokio::select! {
                r = db::execute_pg(&mut conn, &sql, vec![]) => r,
                _ = abort_rx => Err(CANCELLED.to_string()),
            }
        }
        DbPool::Sqlite(_) => {
            state.running.lock().await.insert(
                query_id.clone(),
                RunningQuery {
                    conn_id: id.clone(),
                    pg_pid: None,
                    abort,
                },
            );
            tokio::select! {
                r = db::execute(&pool, &sql, vec![]) => r,
                _ = abort_rx => Err(CANCELLED.to_string()),
            }
        }
    };
    state.running.lock().await.remove(&query_id);
    out
}

#[tauri::command]
pub async fn cancel_query(state: State<'_, AppState>, query_id: String) -> CmdResult<()> {
    let Some(q) = state.running.lock().await.remove(&query_id) else {
        return Ok(()); // already finished
    };
    // Stop the statement server-side first. Dropping our future alone would
    // free the tab but leave Postgres burning CPU on the query.
    if let Some(pid) = q.pg_pid {
        let pool = state.pools.lock().await.get(&q.conn_id).cloned();
        if let Some(pool) = pool {
            if let DbPool::Pg(p) = &*pool {
                sqlx::query("SELECT pg_cancel_backend($1)")
                    .bind(pid)
                    .execute(p)
                    .await
                    .map_err(err)?;
            }
        }
    }
    // SQLite has no server to ask, so there the dropped future is the whole
    // mechanism: the tab frees up and the connection is discarded.
    let _ = q.abort.send(()); // Err just means the query already returned
    Ok(())
}

fn pk_column(schema: &[ColumnInfo]) -> CmdResult<&ColumnInfo> {
    let pks: Vec<_> = schema.iter().filter(|c| c.is_pk).collect();
    match pks.len() {
        1 => Ok(pks[0]),
        0 => Err("table has no primary key — editing disabled".into()),
        _ => Err("composite primary keys not supported".into()),
    }
}

fn ph(pg: bool, n: usize) -> String {
    if pg {
        format!("${n}")
    } else {
        "?".to_string()
    }
}

#[tauri::command]
pub async fn update_cell(
    state: State<'_, AppState>,
    id: String,
    table: String,
    column: String,
    value: Value,
    pk_value: Value,
) -> CmdResult<u64> {
    with_pool!(state, id, pool => {
        let schema = fetch_schema(pool, &table).await?;
        let pk = pk_column(&schema)?;
        if !schema.iter().any(|c| c.name == column) {
            return Err(format!("unknown column: {column}"));
        }
        let pg = matches!(pool, DbPool::Pg(_));
        let col_type = schema
            .iter()
            .find(|c| c.name == column)
            .map(|c| c.data_type.as_str())
            .unwrap_or("text");
        let sql = format!(
            "UPDATE {} SET {} = {} WHERE {} = {}",
            db::quote_ident(&table),
            db::quote_ident(&column),
            db::cast_ph(pg, &ph(pg, 1), col_type),
            db::quote_ident(&pk.name),
            db::cast_ph(pg, &ph(pg, 2), &pk.data_type),
        );
        let binds = vec![Bind::from_json(&value)?, Bind::from_json(&pk_value)?];
        Ok(db::execute(pool, &sql, binds).await?.rows_affected)
    })
}

#[tauri::command]
pub async fn insert_row(
    state: State<'_, AppState>,
    id: String,
    table: String,
    values: HashMap<String, Value>,
) -> CmdResult<u64> {
    with_pool!(state, id, pool => {
        let schema = fetch_schema(pool, &table).await?;
        let pg = matches!(pool, DbPool::Pg(_));
        let mut cols = vec![];
        let mut phs = vec![];
        let mut binds = vec![];
        for (i, (col, val)) in values.iter().enumerate() {
            let Some(info) = schema.iter().find(|c| &c.name == col) else {
                return Err(format!("unknown column: {col}"));
            };
            cols.push(db::quote_ident(col));
            phs.push(db::cast_ph(pg, &ph(pg, i + 1), &info.data_type));
            binds.push(Bind::from_json(val)?);
        }
        if cols.is_empty() {
            return Err("no values".into());
        }
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            db::quote_ident(&table),
            cols.join(", "),
            phs.join(", "),
        );
        Ok(db::execute(pool, &sql, binds).await?.rows_affected)
    })
}

#[tauri::command]
pub async fn delete_row(
    state: State<'_, AppState>,
    id: String,
    table: String,
    pk_value: Value,
) -> CmdResult<u64> {
    with_pool!(state, id, pool => {
        let schema = fetch_schema(pool, &table).await?;
        let pk = pk_column(&schema)?;
        let pg = matches!(pool, DbPool::Pg(_));
        let sql = format!(
            "DELETE FROM {} WHERE {} = {}",
            db::quote_ident(&table),
            db::quote_ident(&pk.name),
            db::cast_ph(pg, &ph(pg, 1), &pk.data_type),
        );
        Ok(db::execute(pool, &sql, vec![Bind::from_json(&pk_value)?]).await?.rows_affected)
    })
}

// ---------- ddl ----------

#[tauri::command]
pub async fn add_column(
    state: State<'_, AppState>,
    id: String,
    table: String,
    name: String,
    col_type: String,
    nullable: bool,
    default_value: Option<String>,
) -> CmdResult<()> {
    if !db::valid_ident(&name) {
        return Err("invalid column name".into());
    }
    // type is interpolated into DDL: letters/digits/space/paren/comma only
    if col_type.is_empty()
        || !col_type
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " (),_".contains(c))
    {
        return Err("invalid column type".into());
    }
    with_pool!(state, id, pool => {
        fetch_schema(pool, &table).await?; // validates table exists
        let mut sql = format!(
            "ALTER TABLE {} ADD COLUMN {} {}",
            db::quote_ident(&table),
            db::quote_ident(&name),
            col_type,
        );
        if !nullable {
            sql.push_str(" NOT NULL");
        }
        if let Some(d) = default_value.filter(|d| !d.is_empty()) {
            // DDL can't bind params: numbers raw, anything else as escaped string literal
            if d.parse::<f64>().is_ok() {
                sql.push_str(&format!(" DEFAULT {d}"));
            } else {
                sql.push_str(&format!(" DEFAULT '{}'", d.replace('\'', "''")));
            }
        }
        db::execute(pool, &sql, vec![]).await?;
        Ok(())
    })
}

// ---------- saved queries ----------

#[tauri::command]
pub async fn list_saved_queries(
    state: State<'_, AppState>,
    connection_id: String,
) -> CmdResult<Vec<SavedQuery>> {
    sqlx::query_as::<_, SavedQuery>(
        "SELECT * FROM saved_queries WHERE connection_id = ? ORDER BY name",
    )
    .bind(&connection_id)
    .fetch_all(&state.meta)
    .await
    .map_err(err)
}

#[tauri::command]
pub async fn save_query(state: State<'_, AppState>, mut query: SavedQuery) -> CmdResult<SavedQuery> {
    if query.id.is_empty() {
        query.id = uuid::Uuid::new_v4().to_string();
    }
    sqlx::query(
        "INSERT INTO saved_queries (id, connection_id, name, sql) VALUES (?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, sql=excluded.sql",
    )
    .bind(&query.id)
    .bind(&query.connection_id)
    .bind(&query.name)
    .bind(&query.sql)
    .execute(&state.meta)
    .await
    .map_err(err)?;
    Ok(query)
}

#[tauri::command]
pub async fn delete_saved_query(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    sqlx::query("DELETE FROM saved_queries WHERE id = ?")
        .bind(&id)
        .execute(&state.meta)
        .await
        .map_err(err)?;
    Ok(())
}

// ---------- export ----------

#[tauri::command]
pub async fn export_rows(
    state: State<'_, AppState>,
    id: String,
    sql: String,
    format: String,
    path: String,
) -> CmdResult<u64> {
    let result = with_pool!(state, id, pool => db::execute(pool, &sql, vec![]).await)?;
    let n = result.rows.len() as u64;
    match format.as_str() {
        "csv" => {
            let mut w = csv::Writer::from_path(&path).map_err(err)?;
            w.write_record(&result.columns).map_err(err)?;
            for row in &result.rows {
                let rec: Vec<String> = row.iter().map(cell_text).collect();
                w.write_record(&rec).map_err(err)?;
            }
            w.flush().map_err(err)?;
        }
        "json" => {
            let objs: Vec<serde_json::Map<String, Value>> = result
                .rows
                .iter()
                .map(|row| {
                    result
                        .columns
                        .iter()
                        .cloned()
                        .zip(row.iter().cloned())
                        .collect()
                })
                .collect();
            let f = std::fs::File::create(&path).map_err(err)?;
            serde_json::to_writer_pretty(f, &objs).map_err(err)?;
        }
        other => return Err(format!("unknown format: {other}")),
    }
    Ok(n)
}

fn cell_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
