use crate::db::{
    self, Bind, ColumnInfo, DbPool, Dialect, Filter, ForeignKey, QueryResult, Sort, TableRef,
};
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
    /// The server's handle for this session, so another one can stop the
    /// statement. `None` where the engine has no server to ask.
    backend: Option<Backend>,
    /// Fires to drop the in-flight future, unblocking the tab.
    abort: oneshot::Sender<()>,
}

pub enum Backend {
    /// backend PID, for `pg_cancel_backend`
    Pg(i32),
    /// connection id, for `KILL QUERY`
    MySql(u64),
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
    with_pool!(state, id, pool => fetch_tables(pool).await)
}

async fn fetch_tables(pool: &DbPool) -> CmdResult<Vec<TableRef>> {
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
        DbPool::My(_) => {
            // MySQL's "schema" is the database itself, so there is only ever
            // one to look in and a bare table name is unambiguous.
            // information_schema hands its strings back as VARBINARY, which the
            // decoder turns into a hex dump unless it is cast to CHAR first.
            let res = db::execute(
                pool,
                "SELECT CAST(table_name AS CHAR) FROM information_schema.tables
                 WHERE table_schema = DATABASE() AND table_type = 'BASE TABLE'
                 ORDER BY 1",
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
        DbPool::Mssql(p) => crate::mssql::tables(p).await,
    }
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
            .bind(table.quoted(Dialect::Pg))
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
                Dialect::Sqlite.quote_ident(&table.name)
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
        DbPool::My(p) => {
            // COLUMN_TYPE, not DATA_TYPE: it keeps the length and the unsigned
            // flag, so the string stays usable as a CAST target
            let cols = sqlx::query(
                "SELECT CAST(column_name AS CHAR), CAST(column_type AS CHAR),
                        CAST(is_nullable AS CHAR), CAST(column_key AS CHAR)
                 FROM information_schema.columns
                 WHERE table_schema = DATABASE() AND table_name = ?
                 ORDER BY ordinal_position",
            )
            .bind(&table.name)
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
                    nullable: r.get::<String, _>(2) == "YES",
                    is_pk: r.get::<String, _>(3) == "PRI",
                })
                .collect())
        }
        DbPool::Mssql(p) => crate::mssql::schema(p, table).await,
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
            .bind(table.quoted(Dialect::Pg))
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
                Dialect::Sqlite.quote_ident(&table.name)
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
        DbPool::My(p) => {
            let rows = sqlx::query(
                "SELECT CAST(constraint_name AS CHAR), CAST(column_name AS CHAR),
                        CAST(referenced_table_name AS CHAR),
                        CAST(referenced_column_name AS CHAR)
                 FROM information_schema.key_column_usage
                 WHERE table_schema = DATABASE()
                   AND table_name = ?
                   AND referenced_table_name IS NOT NULL
                 ORDER BY constraint_name, ordinal_position",
            )
            .bind(&table.name)
            .fetch_all(p)
            .await
            .map_err(err)?;
            // one row per column: a constraint appearing twice is a composite key
            let mut counts: HashMap<String, usize> = HashMap::new();
            for r in &rows {
                *counts.entry(r.get::<String, _>(0)).or_default() += 1;
            }
            Ok(rows
                .iter()
                .filter(|r| counts[&r.get::<String, _>(0)] == 1)
                .map(|r| ForeignKey {
                    column: r.get(1),
                    ref_schema: None,
                    ref_table: r.get(2),
                    ref_column: r.get(3),
                })
                .collect())
        }
        DbPool::Mssql(p) => crate::mssql::foreign_keys(p, table).await,
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
            pool.dialect(),
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
        let (sql, binds) = db::build_count(&table, &schema, &filters, pool.dialect())?;
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
                    backend: Some(Backend::Pg(pid)),
                    abort,
                },
            );
            tokio::select! {
                r = db::execute_pg(&mut conn, &sql, vec![]) => r,
                _ = abort_rx => Err(CANCELLED.to_string()),
            }
        }
        DbPool::My(p) => {
            let mut conn = p.acquire().await.map_err(err)?;
            let cid: u64 = sqlx::query_scalar("SELECT CONNECTION_ID()")
                .fetch_one(&mut *conn)
                .await
                .map_err(err)?;
            state.running.lock().await.insert(
                query_id.clone(),
                RunningQuery {
                    conn_id: id.clone(),
                    backend: Some(Backend::MySql(cid)),
                    abort,
                },
            );
            tokio::select! {
                r = db::execute_mysql(&mut conn, &sql, vec![]) => r,
                _ = abort_rx => Err(CANCELLED.to_string()),
            }
        }
        // ponytail: tiberius exposes no cancel token, so SQL Server gets the
        // SQLite treatment — dropping the future frees the tab but leaves the
        // server working. Revisit if tiberius grows an attention request.
        DbPool::Sqlite(_) | DbPool::Mssql(_) => {
            state.running.lock().await.insert(
                query_id.clone(),
                RunningQuery {
                    conn_id: id.clone(),
                    backend: None,
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
    // free the tab but leave the server burning CPU on the query.
    if let Some(backend) = &q.backend {
        let pool = state.pools.lock().await.get(&q.conn_id).cloned();
        if let Some(pool) = pool {
            match (backend, &*pool) {
                (Backend::Pg(pid), DbPool::Pg(p)) => {
                    sqlx::query("SELECT pg_cancel_backend($1)")
                        .bind(pid)
                        .execute(p)
                        .await
                        .map_err(err)?;
                }
                // KILL takes no bind parameters; the id came from the server
                (Backend::MySql(cid), DbPool::My(p)) => {
                    sqlx::query(&format!("KILL QUERY {cid}"))
                        .execute(p)
                        .await
                        .map_err(err)?;
                }
                _ => {}
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
    d: Dialect,
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
            d.quote_ident(&c.name),
            d.cast_ph(&d.placeholder(start + i), &c.data_type),
        ));
        binds.push(Bind::from_json(&pk_values[&c.name])?);
    }
    Ok((parts.join(" AND "), binds))
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
        let d = pool.dialect();
        let (where_sql, pk_binds) = pk_where(&schema, &pk_values, d, 2)?;
        let sql = format!(
            "UPDATE {} SET {} = {} WHERE {}",
            table.quoted(d),
            d.quote_ident(&column),
            d.cast_ph(&d.placeholder(1), &col.data_type),
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
        let d = pool.dialect();
        let mut cols = vec![];
        let mut phs = vec![];
        let mut binds = vec![];
        for (i, (col, val)) in values.iter().enumerate() {
            let Some(info) = schema.iter().find(|c| &c.name == col) else {
                return Err(format!("unknown column: {col}"));
            };
            cols.push(d.quote_ident(col));
            phs.push(d.cast_ph(&d.placeholder(i + 1), &info.data_type));
            binds.push(Bind::from_json(val)?);
        }
        if cols.is_empty() {
            return Err("no values".into());
        }
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table.quoted(d),
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
        let d = pool.dialect();
        let (where_sql, binds) = pk_where(&schema, &pk_values, d, 1)?;
        let sql = format!(
            "DELETE FROM {} WHERE {}",
            table.quoted(d),
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
        let d = pool.dialect();
        let mut sql = format!(
            "ALTER TABLE {} {} {} {}",
            table.quoted(d),
            d.add_column_kw(),
            d.quote_ident(&name),
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
            pool.dialect(),
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

    /// Everything the MySQL arms do that a unit test cannot reach: real
    /// introspection, real decoding, real generated SQL. Ignored by default
    /// because it needs a server; see the MySQL section of the README.
    ///
    ///   docker run -d --name dbelte-mysql -e MYSQL_ROOT_PASSWORD=dbelte \
    ///     -e MYSQL_DATABASE=shop -p 13306:3306 mysql:8
    ///   cargo test --lib mysql -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "needs a MySQL server on 127.0.0.1:13306"]
    async fn talks_to_a_real_mysql() {
        let conn = Connection {
            id: String::new(),
            name: "t".into(),
            engine: "mysql".into(),
            host: Some("127.0.0.1".into()),
            port: Some(13306),
            database: "shop".into(),
            username: Some("root".into()),
        };
        let pool = db::open(&conn, Some("dbelte".into())).await.expect("connect");

        let tables = fetch_tables(&pool).await.expect("list tables");
        let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["customers", "line_items", "orders"]);
        assert!(tables.iter().all(|t| t.schema.is_none()), "the database is the schema");

        let customers = TableRef { schema: None, name: "customers".into() };
        let schema = fetch_schema(&pool, &customers).await.expect("schema");
        let col = |n: &str| schema.iter().find(|c| c.name == n).unwrap_or_else(|| panic!("no {n}"));
        assert!(col("id").is_pk);
        assert!(!col("id").nullable);
        assert!(col("credit").nullable);
        // COLUMN_TYPE keeps the length, which DATA_TYPE would have thrown away
        assert_eq!(col("name").data_type, "varchar(120)");
        assert_eq!(col("credit").data_type, "decimal(10,2)");
        assert!(fetch_schema(&pool, &TableRef { schema: None, name: "nope".into() })
            .await
            .is_err());

        // single-column FK is reported, composite PK table has none to report
        let fks = fetch_fks(&pool, &TableRef { schema: None, name: "orders".into() })
            .await
            .expect("fks");
        assert_eq!(fks.len(), 1);
        assert_eq!(fks[0].column, "customer_id");
        assert_eq!(fks[0].ref_table, "customers");
        assert_eq!(fks[0].ref_column, "id");

        // backtick quoting, `?` placeholders and LIKE-escaping, end to end
        let filters = vec![Filter {
            column: "name".into(),
            op: "contains".into(),
            value: "o%b_".into(),
        }];
        let (sql, binds) =
            db::build_select(&customers, &schema, &filters, None, 10, 0, pool.dialect()).unwrap();
        assert!(sql.starts_with("SELECT * FROM `customers`"), "{sql}");
        let rows = db::execute(&pool, &sql, binds).await.expect("filtered select");
        assert_eq!(rows.rows.len(), 1, "the user's % and _ are literals, not wildcards");

        // every column type the decoder branches on
        let all = db::execute(&pool, "SELECT * FROM customers ORDER BY id", vec![])
            .await
            .expect("select all");
        let at = |name: &str, row: usize| -> &serde_json::Value {
            let i = all.columns.iter().position(|c| c == name).unwrap();
            &all.rows[row][i]
        };
        assert_eq!(at("vip", 0), &json!(true), "tinyint(1) reads back as a bool");
        assert_eq!(at("credit", 0), &json!("50.25"), "decimals keep their exact text");
        assert_eq!(at("joined", 0), &json!("2024-01-02 03:04:05"));
        assert_eq!(at("meta", 0), &json!({"tier": "gold"}));
        assert_eq!(at("avatar", 0), &json!("\\x0102"));
        assert_eq!(at("credit", 1), &json!(null));

        // BIGINT UNSIGNED and DOUBLE live on orders
        let o = db::execute(&pool, "SELECT id, total, placed FROM orders", vec![])
            .await
            .expect("select orders");
        assert_eq!(o.rows[0][0], json!(1));
        assert_eq!(o.rows[0][1], json!(9.99));
        assert_eq!(o.rows[0][2], json!("2024-05-06"));

        // DML: the placeholder and cast rules have to hold for writes too
        let n = db::execute(
            &pool,
            "INSERT INTO customers (name, vip) VALUES (?, ?)",
            vec![Bind::Text("Cy".into()), Bind::Bool(true)],
        )
        .await
        .expect("insert")
        .rows_affected;
        assert_eq!(n, 1);
        db::execute(&pool, "DELETE FROM customers WHERE name = ?", vec![Bind::Text("Cy".into())])
            .await
            .expect("delete");

        // the two statements cancellation leans on
        let pool_ref = match &pool {
            DbPool::My(p) => p,
            _ => unreachable!(),
        };
        let cid: u64 = sqlx::query_scalar("SELECT CONNECTION_ID()")
            .fetch_one(pool_ref)
            .await
            .expect("connection id");
        sqlx::query(&format!("KILL QUERY {cid}"))
            .execute(pool_ref)
            .await
            .expect("KILL QUERY on an idle session is a no-op, not an error");
    }

    /// The Postgres twin of the MySQL test above. Mostly a regression net for
    /// changes made for another engine — the LIKE escaping in particular is
    /// shared by all of them.
    ///
    ///   docker run -d --name dbelte-pg -e POSTGRES_PASSWORD=dbelte \\
    ///     -e POSTGRES_DB=shop -p 15432:5432 postgres:16
    ///   cargo test --lib postgres -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "needs a Postgres server on 127.0.0.1:15432"]
    async fn talks_to_a_real_postgres() {
        let conn = Connection {
            id: String::new(),
            name: "t".into(),
            engine: "postgres".into(),
            host: Some("127.0.0.1".into()),
            port: Some(15432),
            database: "shop".into(),
            username: Some("postgres".into()),
        };
        let pool = db::open(&conn, Some("dbelte".into())).await.expect("connect");

        let tables = fetch_tables(&pool).await.expect("list tables");
        assert!(tables.iter().all(|t| t.schema.as_deref() == Some("public")));

        let customers = TableRef { schema: Some("public".into()), name: "customers".into() };
        let schema = fetch_schema(&pool, &customers).await.expect("schema");
        assert!(schema.iter().find(|c| c.name == "id").unwrap().is_pk);

        let fks = fetch_fks(&pool, &TableRef { schema: Some("public".into()), name: "orders".into() })
            .await
            .expect("fks");
        assert_eq!(fks.len(), 1);
        assert_eq!(fks[0].ref_table, "customers");

        // the user's % and _ stay literal under the shared ESCAPE character
        let filters = vec![Filter {
            column: "name".into(),
            op: "contains".into(),
            value: "o%b_".into(),
        }];
        let (sql, binds) =
            db::build_select(&customers, &schema, &filters, None, 10, 0, pool.dialect()).unwrap();
        assert!(sql.starts_with(r#"SELECT * FROM "public"."customers""#), "{sql}");
        let rows = db::execute(&pool, &sql, binds).await.expect("filtered select");
        assert_eq!(rows.rows.len(), 1);

        let all = db::execute(&pool, "SELECT credit FROM customers ORDER BY id", vec![])
            .await
            .expect("select");
        assert_eq!(all.rows[0][0], json!("50.25"));
        assert_eq!(all.rows[1][0], json!(null));
    }

    /// SQL Server goes through tiberius, not sqlx, so none of the shared
    /// machinery covers it — this is the only proof the arm works.
    ///
    ///   docker run -d --name dbelte-mssql -e ACCEPT_EULA=Y \\
    ///     -e MSSQL_SA_PASSWORD='Dbelte!Pass1' -p 11433:1433 \\
    ///     mcr.microsoft.com/mssql/server:2022-latest
    ///   cargo test --lib mssql -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "needs a SQL Server on 127.0.0.1:11433"]
    async fn talks_to_a_real_mssql() {
        let conn = Connection {
            id: String::new(),
            name: "t".into(),
            engine: "mssql".into(),
            host: Some("127.0.0.1".into()),
            port: Some(11433),
            database: "shop".into(),
            username: Some("sa".into()),
        };
        let pool = db::open(&conn, Some("Dbelte!Pass1".into()))
            .await
            .expect("connect");

        // A blank database is legitimate here: SQL Server falls back to the
        // login's default. The connection form relies on this to let the field
        // be left empty, so it is asserted rather than assumed.
        let blank = Connection { database: String::new(), ..conn.clone() };
        let blank_pool = db::open(&blank, Some("Dbelte!Pass1".into()))
            .await
            .expect("connect with no database named");
        let landed = db::execute(&blank_pool, "SELECT DB_NAME()", vec![])
            .await
            .expect("DB_NAME()");
        assert_eq!(landed.rows[0][0], json!("master"));

        // real schemas, like Postgres — dbo is the default, not the only one
        let tables = fetch_tables(&pool).await.expect("list tables");
        let pairs: Vec<String> = tables
            .iter()
            .map(|t| format!("{}.{}", t.schema.as_deref().unwrap_or(""), t.name))
            .collect();
        assert_eq!(pairs, ["dbo.customers", "dbo.orders", "sales.regions"]);

        let customers = TableRef { schema: Some("dbo".into()), name: "customers".into() };
        let schema = fetch_schema(&pool, &customers).await.expect("schema");
        let col = |n: &str| schema.iter().find(|c| c.name == n).unwrap_or_else(|| panic!("no {n}"));
        assert!(col("id").is_pk);
        assert!(!col("id").nullable);
        assert!(col("credit").nullable);
        // the length/precision suffix has to survive: it is the CAST target
        assert_eq!(col("name").data_type, "nvarchar(120)");
        assert_eq!(col("credit").data_type, "decimal(10,2)");
        assert_eq!(col("avatar").data_type, "varbinary(max)");
        assert!(fetch_schema(&pool, &TableRef { schema: Some("dbo".into()), name: "nope".into() })
            .await
            .is_err());

        let fks = fetch_fks(&pool, &TableRef { schema: Some("dbo".into()), name: "orders".into() })
            .await
            .expect("fks");
        assert_eq!(fks.len(), 1);
        assert_eq!(fks[0].column, "customer_id");
        assert_eq!(fks[0].ref_schema.as_deref(), Some("dbo"));
        assert_eq!(fks[0].ref_table, "customers");

        // [brackets], @P1 and NVARCHAR(MAX) as the text cast target
        let filters = vec![Filter {
            column: "name".into(),
            op: "contains".into(),
            value: "o%b_".into(),
        }];
        let (sql, binds) =
            db::build_select(&customers, &schema, &filters, None, 10, 0, pool.dialect()).unwrap();
        assert!(sql.starts_with("SELECT * FROM [dbo].[customers]"), "{sql}");
        assert!(sql.contains("AS NVARCHAR(MAX)) LIKE @P1"), "{sql}");
        // OFFSET…FETCH is a syntax error without an ORDER BY, hence the stub
        assert!(sql.contains("ORDER BY (SELECT NULL) OFFSET 0 ROWS"), "{sql}");
        let rows = db::execute(&pool, &sql, binds).await.expect("filtered select");
        assert_eq!(rows.rows.len(), 1, "the user's % and _ are literals, not wildcards");

        // paging with a real sort must not pick up the stub
        let sort = Sort { column: "id".into(), desc: false };
        let (paged, _) =
            db::build_select(&customers, &schema, &[], Some(&sort), 1, 1, pool.dialect()).unwrap();
        assert!(!paged.contains("SELECT NULL"), "{paged}");
        let second = db::execute(&pool, &paged, vec![]).await.expect("page 2");
        assert_eq!(second.rows.len(), 1);

        // every ColumnData branch the decoder covers
        let all = db::execute(&pool, "SELECT * FROM [dbo].[customers] ORDER BY id", vec![])
            .await
            .expect("select all");
        let at = |name: &str, row: usize| -> &serde_json::Value {
            let i = all.columns.iter().position(|c| c == name).unwrap();
            &all.rows[row][i]
        };
        assert_eq!(at("vip", 0), &json!(true), "BIT reads back as a bool");
        assert_eq!(at("credit", 0), &json!("50.25"), "decimals keep their exact text");
        assert_eq!(at("joined", 0), &json!("2024-01-02 03:04:05"));
        assert_eq!(
            at("ref", 0),
            &json!("11111111-2222-3333-4444-555555555555"),
            "uniqueidentifier"
        );
        assert_eq!(at("avatar", 0), &json!("\\x0102"));
        assert_eq!(at("credit", 1), &json!(null));

        let o = db::execute(&pool, "SELECT id, total, placed FROM [dbo].[orders]", vec![])
            .await
            .expect("select orders");
        assert_eq!(o.rows[0][0], json!(1));
        assert_eq!(o.rows[0][1], json!(9.99));
        assert_eq!(o.rows[0][2], json!("2024-05-06"));

        // writes: rows_affected has to come back from ExecuteResult, not a row set
        let n = db::execute(
            &pool,
            "INSERT INTO [dbo].[customers] (name, vip) VALUES (@P1, @P2)",
            vec![Bind::Text("Cy".into()), Bind::Bool(true)],
        )
        .await
        .expect("insert")
        .rows_affected;
        assert_eq!(n, 1);
        let d = db::execute(
            &pool,
            "DELETE FROM [dbo].[customers] WHERE name = @P1",
            vec![Bind::Text("Cy".into())],
        )
        .await
        .expect("delete")
        .rows_affected;
        assert_eq!(d, 1);
    }

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
        let (sql, binds) = pk_where(&two_pk_schema(), &vals, Dialect::Pg, 2).unwrap();
        assert_eq!(sql, r#""a" = CAST($2 AS integer) AND "b" = CAST($3 AS text)"#);
        assert_eq!(binds.len(), 2);
    }

    #[test]
    fn partial_key_is_refused() {
        // half a composite key would match many rows — never build that WHERE
        let vals = HashMap::from([("a".to_string(), json!(1))]);
        assert!(pk_where(&two_pk_schema(), &vals, Dialect::Pg, 1).is_err());
        assert!(pk_where(&[], &HashMap::new(), Dialect::Pg, 1).is_err());
    }
}
