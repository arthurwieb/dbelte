//! The SQL Server driver. Separate from `db.rs` because sqlx has no MSSQL
//! driver — it was dropped after 0.6 — so this path shares no traits with the
//! other engines: no `sqlx::Row`, no `sqlx::query`, no `bind_all!`. What it does
//! share is the output. Everything here produces the same `QueryResult` and
//! `ColumnInfo` the rest of the app already understands, which is the seam that
//! makes SQL Server just another engine above this file.

use crate::db::{Bind, ColumnInfo, ForeignKey, QueryResult, TableRef};
use serde_json::{json, Value};
use tiberius::{ColumnData, ToSql};

pub type Pool = deadpool_tiberius::Pool;

pub async fn connect(
    host: &str,
    port: u16,
    database: &str,
    username: &str,
    password: &str,
) -> Result<Pool, String> {
    // `host\instance`: a named instance listens on its own dynamic port, which
    // the SQL Browser service resolves from the name
    let (host, instance) = match host.split_once('\\') {
        Some((h, i)) => (h, Some(i)),
        None => (host, None),
    };
    let mut cfg = deadpool_tiberius::Manager::new()
        .host(host)
        .port(port)
        .database(database)
        .basic_authentication(username, password)
        // SQL Server ships with a self-signed certificate and almost nobody
        // replaces it, so refusing it would reject most real servers
        .trust_cert()
        .max_size(4)
        .wait_timeout(std::time::Duration::from_secs(10));
    if let Some(name) = instance {
        cfg = cfg.instance_name(name).enable_sql_browser();
    }
    cfg.create_pool().map_err(|e| e.to_string())
}

/// `Bind` as something tiberius will send. Kept owned and separate from the
/// `&[&dyn ToSql]` slice `query` wants, because that slice has to borrow from
/// values that outlive the call.
fn to_sql(b: &Bind) -> &dyn ToSql {
    match b {
        Bind::Null => &Option::<&str>::None,
        Bind::Bool(v) => v,
        Bind::Int(v) => v,
        Bind::Float(v) => v,
        Bind::Text(v) => v,
    }
}

pub async fn execute(pool: &Pool, sql: &str, binds: Vec<Bind>) -> Result<QueryResult, String> {
    let mut client = pool.get().await.map_err(|e| e.to_string())?;
    let params: Vec<&dyn ToSql> = binds.iter().map(to_sql).collect();
    if is_fetch(sql) {
        let rows = client
            .query(sql, &params)
            .await
            .map_err(|e| e.to_string())?
            .into_first_result()
            .await
            .map_err(|e| e.to_string())?;
        Ok(rows_to_result(&rows))
    } else {
        let res = client
            .execute(sql, &params)
            .await
            .map_err(|e| e.to_string())?;
        Ok(QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: res.total(),
        })
    }
}

/// T-SQL's own statement classification. `db::is_fetch` covers the keywords
/// shared with the other engines; this adds the ones only SQL Server has and
/// drops `RETURNING`, which it spells `OUTPUT`.
fn is_fetch(sql: &str) -> bool {
    let head = sql.trim_start().to_ascii_lowercase();
    ["select", "with", "exec", "execute", "declare", "print"]
        .iter()
        .any(|k| head.starts_with(k))
        || head.contains(" output ")
}

fn rows_to_result(rows: &[tiberius::Row]) -> QueryResult {
    let columns = rows
        .first()
        .map(|r| r.columns().iter().map(|c| c.name().to_string()).collect())
        .unwrap_or_default();
    let data = rows
        .iter()
        .map(|row| row.cells().map(|(_, v)| cell(v)).collect())
        .collect();
    QueryResult {
        columns,
        rows: data,
        rows_affected: 0,
    }
}

/// Unlike the sqlx engines, tiberius hands back an already-decoded value, so
/// this matches on the data rather than guessing from a type name.
fn cell(v: &ColumnData<'static>) -> Value {
    match v {
        ColumnData::U8(x) => json!(x),
        ColumnData::I16(x) => json!(x),
        ColumnData::I32(x) => json!(x),
        ColumnData::I64(x) => json!(x),
        ColumnData::F32(x) => json!(x),
        ColumnData::F64(x) => json!(x),
        ColumnData::Bit(x) => json!(x),
        ColumnData::String(x) => json!(x),
        ColumnData::Guid(x) => json!(x.map(|g| g.to_string())),
        // exact text, the same as NUMERIC on Postgres — a float would round it
        ColumnData::Numeric(x) => json!(x.map(|n| n.to_string())),
        ColumnData::Xml(x) => json!(x.as_ref().map(|d| d.to_string())),
        ColumnData::Binary(x) => json!(x.as_ref().map(|b| format!("\\x{}", hex_str(b)))),
        // the temporal types carry raw TDS counters, so let tiberius' FromSql
        // turn them into chrono values rather than decoding the wire format here
        ColumnData::DateTime(_)
        | ColumnData::SmallDateTime(_)
        | ColumnData::Time(_)
        | ColumnData::Date(_)
        | ColumnData::DateTime2(_)
        | ColumnData::DateTimeOffset(_) => temporal(v),
    }
}

fn temporal(v: &ColumnData<'static>) -> Value {
    use tiberius::time::chrono::{NaiveDate, NaiveDateTime, NaiveTime};
    macro_rules! try_as {
        ($t:ty, $fmt:expr) => {
            if let Ok(x) = <$t as tiberius::FromSql>::from_sql(v) {
                if let Some(x) = x {
                    return json!($fmt(x));
                }
                return Value::Null;
            }
        };
    }
    try_as!(NaiveDateTime, |d: NaiveDateTime| d.to_string());
    try_as!(NaiveDate, |d: NaiveDate| d.to_string());
    try_as!(NaiveTime, |d: NaiveTime| d.to_string());
    try_as!(
        tiberius::time::chrono::DateTime<tiberius::time::chrono::Utc>,
        |d: tiberius::time::chrono::DateTime<tiberius::time::chrono::Utc>| d.to_rfc3339()
    );
    Value::Null
}

fn hex_str(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------- introspection ----------

pub async fn tables(pool: &Pool) -> Result<Vec<TableRef>, String> {
    // SQL Server has real schemas like Postgres, so the pair is what identifies
    // a table; `dbo` is only the default, not the only one
    let res = execute(
        pool,
        "SELECT s.name, t.name
         FROM sys.tables t
         JOIN sys.schemas s ON s.schema_id = t.schema_id
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

pub async fn schema(pool: &Pool, table: &TableRef) -> Result<Vec<ColumnInfo>, String> {
    // the length/precision suffix is rebuilt here because the CAST targets the
    // filter builder generates are only valid with it
    let res = execute(
        pool,
        "SELECT c.name,
                t.name + CASE
                  WHEN t.name IN ('decimal','numeric')
                    THEN '(' + CAST(c.precision AS varchar(4)) + ','
                             + CAST(c.scale AS varchar(4)) + ')'
                  WHEN t.name IN ('varchar','char','varbinary','binary')
                    THEN '(' + CASE WHEN c.max_length = -1 THEN 'max'
                                    ELSE CAST(c.max_length AS varchar(6)) END + ')'
                  WHEN t.name IN ('nvarchar','nchar')
                    THEN '(' + CASE WHEN c.max_length = -1 THEN 'max'
                                    ELSE CAST(c.max_length / 2 AS varchar(6)) END + ')'
                  ELSE '' END,
                c.is_nullable,
                CAST(CASE WHEN pk.column_id IS NULL THEN 0 ELSE 1 END AS bit)
         FROM sys.columns c
         JOIN sys.tables tb ON tb.object_id = c.object_id
         JOIN sys.schemas s ON s.schema_id = tb.schema_id
         JOIN sys.types t ON t.user_type_id = c.user_type_id
         LEFT JOIN (
             SELECT ic.object_id, ic.column_id
             FROM sys.index_columns ic
             JOIN sys.indexes i
               ON i.object_id = ic.object_id AND i.index_id = ic.index_id
             WHERE i.is_primary_key = 1
         ) pk ON pk.object_id = c.object_id AND pk.column_id = c.column_id
         WHERE s.name = @P1 AND tb.name = @P2
         ORDER BY c.column_id",
        vec![
            Bind::Text(table.schema.clone().unwrap_or_else(|| "dbo".into())),
            Bind::Text(table.name.clone()),
        ],
    )
    .await?;
    if res.rows.is_empty() {
        return Err(format!("unknown table: {table}"));
    }
    Ok(res
        .rows
        .iter()
        .filter_map(|r| {
            Some(ColumnInfo {
                name: r.first()?.as_str()?.to_string(),
                data_type: r.get(1)?.as_str()?.to_string(),
                nullable: r.get(2)?.as_bool().unwrap_or(true),
                is_pk: r.get(3)?.as_bool().unwrap_or(false),
            })
        })
        .collect())
}

pub async fn foreign_keys(pool: &Pool, table: &TableRef) -> Result<Vec<ForeignKey>, String> {
    // single-column keys only, same as the other engines: one equality filter
    // can't express a composite one
    let res = execute(
        pool,
        "SELECT pc.name, rs.name, rt.name, rc.name
         FROM sys.foreign_keys fk
         JOIN sys.foreign_key_columns fkc ON fkc.constraint_object_id = fk.object_id
         JOIN sys.tables pt ON pt.object_id = fk.parent_object_id
         JOIN sys.schemas ps ON ps.schema_id = pt.schema_id
         JOIN sys.columns pc
           ON pc.object_id = fkc.parent_object_id AND pc.column_id = fkc.parent_column_id
         JOIN sys.tables rt ON rt.object_id = fk.referenced_object_id
         JOIN sys.schemas rs ON rs.schema_id = rt.schema_id
         JOIN sys.columns rc
           ON rc.object_id = fkc.referenced_object_id
          AND rc.column_id = fkc.referenced_column_id
         WHERE ps.name = @P1 AND pt.name = @P2
           AND (SELECT count(*) FROM sys.foreign_key_columns
                WHERE constraint_object_id = fk.object_id) = 1",
        vec![
            Bind::Text(table.schema.clone().unwrap_or_else(|| "dbo".into())),
            Bind::Text(table.name.clone()),
        ],
    )
    .await?;
    Ok(res
        .rows
        .iter()
        .filter_map(|r| {
            Some(ForeignKey {
                column: r.first()?.as_str()?.to_string(),
                ref_schema: Some(r.get(1)?.as_str()?.to_string()),
                ref_table: r.get(2)?.as_str()?.to_string(),
                ref_column: r.get(3)?.as_str()?.to_string(),
            })
        })
        .collect())
}
