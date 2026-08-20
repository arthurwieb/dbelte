use crate::db::{self, Bind, ColumnInfo, DbPool, Filter, ForeignKey, QueryResult, Sort, TableRef};
use crate::meta::{self, Connection, HistoryEntry, SavedQuery};
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
pub async fn list_tables(state: State<'_, AppState>, id: String) -> CmdResult<Vec<TableRef>> {
    with_pool!(state, id, pool => {
        match pool {
            DbPool::Pg(_) => {
                // every schema the user put things in, system ones excluded
                let res = db::execute(
                    pool,
                    "SELECT table_schema, table_name FROM information_schema.tables
                     WHERE table_type = 'BASE TABLE'
                       AND table_schema NOT IN ('pg_catalog', 'information_schema')
                       AND table_schema NOT LIKE 'pg_%'
                     ORDER BY 1, 2",
                    vec![],
                )
                .await?;
                Ok(res
                    .rows
                    .into_iter()
                    .filter_map(|r| {
                        Some(TableRef {
                            schema: Some(r.first()?.as_str()?.to_string()),
                            name: r.get(1)?.as_str()?.to_string(),
                        })
                    })
                    .collect())
            }
            DbPool::Sqlite(_) => {
                let res = db::execute(
                    pool,
                    "SELECT name FROM sqlite_master
                     WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY 1",
                    vec![],
                )
                .await?;
                Ok(res
                    .rows
                    .into_iter()
                    .filter_map(|r| {
                        Some(TableRef {
                            schema: None,
                            name: r.first()?.as_str()?.to_string(),
                        })
                    })
                    .collect())
            }
        }
    })
}

async fn fetch_schema(pool: &DbPool, table: &TableRef) -> CmdResult<Vec<ColumnInfo>> {
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
            .bind(table.quoted())
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
            let exists =
                sqlx::query("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?")
                    .bind(&table.name)
                    .fetch_optional(p)
                    .await
                    .map_err(err)?;
            if exists.is_none() {
                return Err(format!("unknown table: {table}"));
            }
            let rows = sqlx::query(&format!(
                "PRAGMA table_info({})",
                db::quote_ident(&table.name)
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
    table: TableRef,
) -> CmdResult<Vec<ColumnInfo>> {
    with_pool!(state, id, pool => fetch_schema(pool, &table).await)
}

/// Single-column foreign keys of a table, so the grid can link a value to the
/// row it points at.
#[tauri::command]
pub async fn foreign_keys(
    state: State<'_, AppState>,
    id: String,
    table: TableRef,
) -> CmdResult<Vec<ForeignKey>> {
    with_pool!(state, id, pool => fetch_fks(pool, &table).await)
}

async fn fetch_fks(pool: &DbPool, table: &TableRef) -> CmdResult<Vec<ForeignKey>> {
    match pool {
        DbPool::Pg(p) => {
            let rows = sqlx::query(
                "SELECT a.attname, cl.relnamespace::regnamespace::text, cl.relname, af.attname
                     FROM pg_constraint c
                     JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = c.conkey[1]
                     JOIN pg_class cl ON cl.oid = c.confrelid
                     JOIN pg_attribute af ON af.attrelid = c.confrelid AND af.attnum = c.confkey[1]
                     WHERE c.contype = 'f'
                       AND c.conrelid = ($1::text)::regclass
                       AND array_length(c.conkey, 1) = 1",
            )
            .bind(table.quoted())
            .fetch_all(p)
            .await
            .map_err(err)?;
            Ok(rows
                .iter()
                .map(|r| ForeignKey {
                    column: r.get(0),
                    ref_schema: r.get(1),
                    ref_table: r.get(2),
                    ref_column: r.get(3),
                })
                .collect())
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(&format!(
                "PRAGMA foreign_key_list({})",
                db::quote_ident(&table.name)
            ))
            .fetch_all(p)
            .await
            .map_err(err)?;
            // one row per column: an id appearing twice is a composite key
            let mut counts: HashMap<i64, usize> = HashMap::new();
            for r in &rows {
                *counts.entry(r.get::<i64, _>("id")).or_default() += 1;
            }
            let mut out = vec![];
            for r in &rows {
                if counts[&r.get::<i64, _>("id")] > 1 {
                    continue;
                }
                let ref_table = TableRef {
                    schema: None,
                    name: r.get("table"),
                };
                // "to" is NULL for `REFERENCES other` — it means other's primary key
                let ref_column = match r.get::<Option<String>, _>("to") {
                    Some(c) => c,
                    None => {
                        let pk = fetch_schema(pool, &ref_table)
                            .await?
                            .into_iter()
                            .find(|c| c.is_pk);
                        match pk {
                            Some(c) => c.name,
                            None => continue,
                        }
                    }
                };
                out.push(ForeignKey {
                    column: r.get("from"),
                    ref_schema: None,
                    ref_table: ref_table.name,
                    ref_column,
                });
            }
            Ok(out)
        }
    }
}

// ---------- data ----------

#[tauri::command]
pub async fn fetch_rows(
    state: State<'_, AppState>,
    id: String,
    table: TableRef,
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

/// Total rows under the current filters, for the pager. Separate from
/// `fetch_rows` because count(*) can be slow on a big table and the grid
/// shouldn't wait for it.
#[tauri::command]
pub async fn count_rows(
    state: State<'_, AppState>,
    id: String,
    table: TableRef,
    filters: Vec<Filter>,
) -> CmdResult<i64> {
    with_pool!(state, id, pool => {
        let schema = fetch_schema(pool, &table).await?;
        let (sql, binds) = db::build_count(&table, &schema, &filters, matches!(pool, DbPool::Pg(_)))?;
        let r = db::execute(pool, &sql, binds).await?;
        Ok(r.rows
            .first()
            .and_then(|row| row.first())
            .and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
            .unwrap_or(0))
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
    if out.is_ok() {
        // history is a convenience, never a reason to fail the query
        let _ = meta::record_history(&state.meta, &id, &sql).await;
    }
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

/// The WHERE clause that names exactly one row, plus its binds. Every primary
/// key column must be present: a partial key matches many rows, and an UPDATE
/// or DELETE that does that is the failure mode worth being paranoid about.
/// `start` is the first placeholder number, since UPDATE binds its value first.
fn pk_where(
    schema: &[ColumnInfo],
    pk_values: &HashMap<String, Value>,
    pg: bool,
    start: usize,
) -> CmdResult<(String, Vec<Bind>)> {
    let pks: Vec<&ColumnInfo> = schema.iter().filter(|c| c.is_pk).collect();
    if pks.is_empty() {
        return Err("table has no primary key — editing disabled".into());
    }
    if pks.len() != pk_values.len() || pks.iter().any(|c| !pk_values.contains_key(&c.name)) {
        return Err("primary key values must name every key column".into());
    }
    let mut parts = vec![];
    let mut binds = vec![];
    for (i, c) in pks.iter().enumerate() {
        parts.push(format!(
            "{} = {}",
            db::quote_ident(&c.name),
            db::cast_ph(pg, &ph(pg, start + i), &c.data_type),
        ));
        binds.push(Bind::from_json(&pk_values[&c.name])?);
    }
    Ok((parts.join(" AND "), binds))
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
    table: TableRef,
    column: String,
    value: Value,
    pk_values: HashMap<String, Value>,
) -> CmdResult<u64> {
    with_pool!(state, id, pool => {
        let schema = fetch_schema(pool, &table).await?;
        let Some(col) = schema.iter().find(|c| c.name == column) else {
            return Err(format!("unknown column: {column}"));
        };
        let pg = matches!(pool, DbPool::Pg(_));
        let (where_sql, pk_binds) = pk_where(&schema, &pk_values, pg, 2)?;
        let sql = format!(
            "UPDATE {} SET {} = {} WHERE {}",
            table.quoted(),
            db::quote_ident(&column),
            db::cast_ph(pg, &ph(pg, 1), &col.data_type),
            where_sql,
        );
        let mut binds = vec![Bind::from_json(&value)?];
        binds.extend(pk_binds);
        Ok(db::execute(pool, &sql, binds).await?.rows_affected)
    })
}

#[tauri::command]
pub async fn insert_row(
    state: State<'_, AppState>,
    id: String,
    table: TableRef,
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
            table.quoted(),
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
    table: TableRef,
    pk_values: HashMap<String, Value>,
) -> CmdResult<u64> {
    with_pool!(state, id, pool => {
        let schema = fetch_schema(pool, &table).await?;
        let pg = matches!(pool, DbPool::Pg(_));
        let (where_sql, binds) = pk_where(&schema, &pk_values, pg, 1)?;
        let sql = format!(
            "DELETE FROM {} WHERE {}",
            table.quoted(),
            where_sql,
        );
        Ok(db::execute(pool, &sql, binds).await?.rows_affected)
    })
}

// ---------- ddl ----------

#[tauri::command]
pub async fn add_column(
    state: State<'_, AppState>,
    id: String,
    table: TableRef,
    name: String,
    col_type: String,
    nullable: bool,
    default_value: Option<String>,
) -> CmdResult<()> {
    if !db::valid_ident(&name) {
        return Err("invalid column name".into());
    }
    // type is interpolated into DDL: letters/digits/space/paren/comma/brackets
    // only — brackets are what makes array types like text[] expressible
    if col_type.is_empty()
        || !col_type
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " (),_[]".contains(c))
    {
        return Err("invalid column type".into());
    }
    with_pool!(state, id, pool => {
        fetch_schema(pool, &table).await?; // validates table exists
        let mut sql = format!(
            "ALTER TABLE {} ADD COLUMN {} {}",
            table.quoted(),
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
pub async fn list_query_history(
    state: State<'_, AppState>,
    connection_id: String,
) -> CmdResult<Vec<HistoryEntry>> {
    sqlx::query_as::<_, HistoryEntry>(
        "SELECT id, sql, ran_at FROM query_history WHERE connection_id = ? ORDER BY id DESC",
    )
    .bind(&connection_id)
    .fetch_all(&state.meta)
    .await
    .map_err(err)
}

#[tauri::command]
pub async fn clear_query_history(
    state: State<'_, AppState>,
    connection_id: String,
) -> CmdResult<()> {
    sqlx::query("DELETE FROM query_history WHERE connection_id = ?")
        .bind(&connection_id)
        .execute(&state.meta)
        .await
        .map_err(err)?;
    Ok(())
}

#[tauri::command]
pub async fn save_query(
    state: State<'_, AppState>,
    mut query: SavedQuery,
) -> CmdResult<SavedQuery> {
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
    write_rows(&result, &format, &path)
}

/// Export a table through the same filter/sort builder the Data tab uses, so
/// what lands in the file is what's on screen — minus the page limit.
#[tauri::command]
pub async fn export_table(
    state: State<'_, AppState>,
    id: String,
    table: TableRef,
    filters: Vec<Filter>,
    sort: Option<Sort>,
    format: String,
    path: String,
) -> CmdResult<u64> {
    let result = with_pool!(state, id, pool => {
        let schema = fetch_schema(pool, &table).await?;
        let (sql, binds) = db::build_select(
            &table,
            &schema,
            &filters,
            sort.as_ref(),
            0, // no LIMIT: export the whole filtered set
            0,
            matches!(pool, DbPool::Pg(_)),
        )?;
        db::execute(pool, &sql, binds).await
    })?;
    write_rows(&result, &format, &path)
}

fn write_rows(result: &QueryResult, format: &str, path: &str) -> CmdResult<u64> {
    let n = result.rows.len() as u64;
    match format {
        "csv" => {
            let mut w = csv::Writer::from_path(path).map_err(err)?;
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
            let f = std::fs::File::create(path).map_err(err)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::Connection;
    use serde_json::json;

    /// sample.db ships with orders.user_id -> users.id, so the SQLite
    /// introspection path has something real to read.
    #[tokio::test]
    async fn reads_sqlite_foreign_keys() {
        let conn = Connection {
            id: "test".into(),
            name: "sample".into(),
            engine: "sqlite".into(),
            host: None,
            port: None,
            database: "../sample.db".into(),
            username: None,
        };
        let pool = db::open(&conn, None).await.expect("open sample.db");
        let t = |name: &str| TableRef { schema: None, name: name.into() };
        let fks = fetch_fks(&pool, &t("orders")).await.expect("read fks");
        assert_eq!(fks.len(), 1);
        assert_eq!(fks[0].column, "user_id");
        assert_eq!(fks[0].ref_table, "users");
        assert_eq!(fks[0].ref_column, "id");
        assert!(fetch_fks(&pool, &t("users")).await.unwrap().is_empty());

        // the same TableRef plumbing drives introspection
        let cols = fetch_schema(&pool, &t("users")).await.expect("read schema");
        assert!(cols.iter().any(|c| c.is_pk), "users should have a primary key");
        assert!(fetch_schema(&pool, &t("nope")).await.is_err());
    }

    fn two_pk_schema() -> Vec<ColumnInfo> {
        vec![
            ColumnInfo { name: "a".into(), data_type: "integer".into(), nullable: false, is_pk: true },
            ColumnInfo { name: "b".into(), data_type: "text".into(), nullable: false, is_pk: true },
            ColumnInfo { name: "v".into(), data_type: "text".into(), nullable: true, is_pk: false },
        ]
    }

    #[test]
    fn composite_key_ands_every_column() {
        let vals = HashMap::from([("a".to_string(), json!(1)), ("b".to_string(), json!("x"))]);
        let (sql, binds) = pk_where(&two_pk_schema(), &vals, true, 2).unwrap();
        assert_eq!(sql, r#""a" = CAST($2 AS integer) AND "b" = CAST($3 AS text)"#);
        assert_eq!(binds.len(), 2);
    }

    #[test]
    fn partial_key_is_refused() {
        // half a composite key would match many rows — never build that WHERE
        let vals = HashMap::from([("a".to_string(), json!(1))]);
        assert!(pk_where(&two_pk_schema(), &vals, true, 1).is_err());
        assert!(pk_where(&[], &HashMap::new(), true, 1).is_err());
    }
}
