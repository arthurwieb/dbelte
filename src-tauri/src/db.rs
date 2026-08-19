use crate::meta::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::postgres::{PgConnectOptions, PgPool, PgRow};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqliteRow};
use sqlx::{Column, Row, TypeInfo};

pub enum DbPool {
    Pg(PgPool),
    Sqlite(SqlitePool),
}

#[derive(Debug, Serialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub rows_affected: u64,
}

#[derive(Debug, Serialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub is_pk: bool,
}

#[derive(Debug, Deserialize)]
pub struct Filter {
    pub column: String,
    pub op: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct Sort {
    pub column: String,
    pub desc: bool,
}

pub async fn open(conn: &Connection, password: Option<String>) -> Result<DbPool, String> {
    match conn.engine.as_str() {
        "postgres" => {
            let mut opts = PgConnectOptions::new()
                .host(conn.host.as_deref().unwrap_or("localhost"))
                .port(conn.port.unwrap_or(5432) as u16)
                .database(&conn.database)
                .username(conn.username.as_deref().unwrap_or("postgres"));
            if let Some(pw) = password.as_deref() {
                opts = opts.password(pw);
            }
            let pool = sqlx::pool::PoolOptions::new()
                .max_connections(4)
                .connect_with(opts)
                .await
                .map_err(|e| e.to_string())?;
            Ok(DbPool::Pg(pool))
        }
        "sqlite" => {
            let opts = SqliteConnectOptions::new()
                .filename(&conn.database)
                .create_if_missing(false);
            let pool = sqlx::pool::PoolOptions::new()
                .max_connections(4)
                .connect_with(opts)
                .await
                .map_err(|e| e.to_string())?;
            Ok(DbPool::Sqlite(pool))
        }
        e => Err(format!("unknown engine: {e}")),
    }
}

/// Double-quote an identifier (valid for both postgres and sqlite).
pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub fn valid_ident(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ' ')
        && !name.starts_with(' ')
}

fn is_fetch(sql: &str) -> bool {
    let head = sql.trim_start().to_ascii_lowercase();
    ["select", "with", "pragma", "explain", "show", "values"]
        .iter()
        .any(|k| head.starts_with(k))
        || head.contains("returning")
}

/// Bindable value derived from a JSON value or a filter string.
#[derive(Debug, Clone)]
pub enum Bind {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl Bind {
    pub fn from_json(v: &Value) -> Result<Bind, String> {
        Ok(match v {
            Value::Null => Bind::Null,
            Value::Bool(b) => Bind::Bool(*b),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Bind::Int(i)
                } else {
                    Bind::Float(n.as_f64().ok_or("bad number")?)
                }
            }
            Value::String(s) => Bind::Text(s.clone()),
            other => Bind::Text(other.to_string()), // arrays/objects as JSON text
        })
    }

    /// Choose a bind type for a filter value based on the column's declared type.
    pub fn for_column(data_type: &str, raw: &str) -> Bind {
        let t = data_type.to_ascii_lowercase();
        if t.contains("int") || t.contains("serial") {
            if let Ok(i) = raw.parse::<i64>() {
                return Bind::Int(i);
            }
        }
        if t.contains("real")
            || t.contains("double")
            || t.contains("float")
            || t.contains("numeric")
            || t.contains("decimal")
        {
            if let Ok(f) = raw.parse::<f64>() {
                return Bind::Float(f);
            }
        }
        if t.contains("bool") {
            if let Ok(b) = raw.parse::<bool>() {
                return Bind::Bool(b);
            }
        }
        Bind::Text(raw.to_string())
    }
}

macro_rules! bind_all {
    ($q:expr, $binds:expr) => {{
        let mut q = $q;
        for b in $binds {
            q = match b {
                Bind::Null => q.bind(None::<String>),
                Bind::Bool(v) => q.bind(v),
                Bind::Int(v) => q.bind(v),
                Bind::Float(v) => q.bind(v),
                Bind::Text(v) => q.bind(v),
            };
        }
        q
    }};
}

pub async fn execute(pool: &DbPool, sql: &str, binds: Vec<Bind>) -> Result<QueryResult, String> {
    if is_fetch(sql) {
        match pool {
            DbPool::Pg(p) => {
                let rows = bind_all!(sqlx::query(sql), binds)
                    .fetch_all(p)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(pg_result(rows))
            }
            DbPool::Sqlite(p) => {
                let rows = bind_all!(sqlx::query(sql), binds)
                    .fetch_all(p)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(sqlite_result(rows))
            }
        }
    } else {
        let affected = match pool {
            DbPool::Pg(p) => bind_all!(sqlx::query(sql), binds)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?
                .rows_affected(),
            DbPool::Sqlite(p) => bind_all!(sqlx::query(sql), binds)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?
                .rows_affected(),
        };
        Ok(QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: affected,
        })
    }
}

/// Run a query on one specific Postgres connection. `execute` takes a pool and
/// may land on any connection in it; cancellation needs the query pinned to the
/// session whose backend PID we handed to `pg_cancel_backend`.
pub async fn execute_pg(
    conn: &mut sqlx::PgConnection,
    sql: &str,
    binds: Vec<Bind>,
) -> Result<QueryResult, String> {
    if is_fetch(sql) {
        let rows = bind_all!(sqlx::query(sql), binds)
            .fetch_all(&mut *conn)
            .await
            .map_err(|e| e.to_string())?;
        Ok(pg_result(rows))
    } else {
        let affected = bind_all!(sqlx::query(sql), binds)
            .execute(&mut *conn)
            .await
            .map_err(|e| e.to_string())?
            .rows_affected();
        Ok(QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: affected,
        })
    }
}

fn pg_result(rows: Vec<PgRow>) -> QueryResult {
    let columns = rows
        .first()
        .map(|r| r.columns().iter().map(|c| c.name().to_string()).collect())
        .unwrap_or_default();
    let data = rows
        .iter()
        .map(|row| (0..row.columns().len()).map(|i| pg_value(row, i)).collect())
        .collect();
    QueryResult {
        columns,
        rows: data,
        rows_affected: 0,
    }
}

fn pg_value(row: &PgRow, i: usize) -> Value {
    let type_name = row.columns()[i].type_info().name().to_string();
    macro_rules! get {
        ($t:ty) => {
            match row.try_get::<Option<$t>, _>(i) {
                Ok(Some(v)) => return json!(v),
                Ok(None) => return Value::Null,
                Err(_) => {}
            }
        };
    }
    match type_name.as_str() {
        "INT2" => get!(i16),
        "INT4" => get!(i32),
        "INT8" => get!(i64),
        "FLOAT4" => get!(f32),
        "FLOAT8" => get!(f64),
        "NUMERIC" => {
            if let Ok(v) = row.try_get::<Option<rust_decimal::Decimal>, _>(i) {
                return v.map(|d| json!(d.to_string())).unwrap_or(Value::Null);
            }
        }
        "BOOL" => get!(bool),
        "UUID" => {
            if let Ok(v) = row.try_get::<Option<uuid::Uuid>, _>(i) {
                return v.map(|u| json!(u.to_string())).unwrap_or(Value::Null);
            }
        }
        "JSON" | "JSONB" => get!(Value),
        "TIMESTAMP" => {
            if let Ok(v) = row.try_get::<Option<chrono::NaiveDateTime>, _>(i) {
                return v.map(|d| json!(d.to_string())).unwrap_or(Value::Null);
            }
        }
        "TIMESTAMPTZ" => {
            if let Ok(v) = row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(i) {
                return v.map(|d| json!(d.to_rfc3339())).unwrap_or(Value::Null);
            }
        }
        "DATE" => {
            if let Ok(v) = row.try_get::<Option<chrono::NaiveDate>, _>(i) {
                return v.map(|d| json!(d.to_string())).unwrap_or(Value::Null);
            }
        }
        "TIME" => {
            if let Ok(v) = row.try_get::<Option<chrono::NaiveTime>, _>(i) {
                return v.map(|d| json!(d.to_string())).unwrap_or(Value::Null);
            }
        }
        "BYTEA" => {
            if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(i) {
                return v
                    .map(|b| json!(format!("\\x{}", hex_str(&b))))
                    .unwrap_or(Value::Null);
            }
        }
        _ => {}
    }
    // fallback: text-ish types and anything unknown
    match row.try_get::<Option<String>, _>(i) {
        Ok(Some(v)) => json!(v),
        Ok(None) => Value::Null,
        Err(_) => json!("<unsupported>"),
    }
}

fn sqlite_result(rows: Vec<SqliteRow>) -> QueryResult {
    let columns = rows
        .first()
        .map(|r| r.columns().iter().map(|c| c.name().to_string()).collect())
        .unwrap_or_default();
    let data = rows
        .iter()
        .map(|row| {
            (0..row.columns().len())
                .map(|i| sqlite_value(row, i))
                .collect()
        })
        .collect();
    QueryResult {
        columns,
        rows: data,
        rows_affected: 0,
    }
}

fn sqlite_value(row: &SqliteRow, i: usize) -> Value {
    // sqlite is dynamically typed: try in order of likelihood
    if let Ok(Some(v)) = row.try_get::<Option<i64>, _>(i) {
        return json!(v);
    }
    if let Ok(Some(v)) = row.try_get::<Option<f64>, _>(i) {
        return json!(v);
    }
    if let Ok(Some(v)) = row.try_get::<Option<String>, _>(i) {
        return json!(v);
    }
    if let Ok(Some(v)) = row.try_get::<Option<Vec<u8>>, _>(i) {
        return json!(format!("\\x{}", hex_str(&v)));
    }
    Value::Null
}

fn hex_str(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

const OPS: &[(&str, &str)] = &[
    ("eq", "="),
    ("neq", "!="),
    ("lt", "<"),
    ("lte", "<="),
    ("gt", ">"),
    ("gte", ">="),
    ("like", "LIKE"),
    ("notlike", "NOT LIKE"),
    ("ilike", "ILIKE"),
    ("notilike", "NOT ILIKE"),
    ("contains", "LIKE"),
    ("startswith", "LIKE"),
    ("endswith", "LIKE"),
    ("in", "IN"),
    ("notin", "NOT IN"),
    ("null", "IS NULL"),
    ("notnull", "IS NOT NULL"),
];

/// Ops whose value is compared as text (the column is CAST first, so patterns
/// work on numeric/date columns too).
const PATTERN_OPS: &[&str] = &[
    "like",
    "notlike",
    "ilike",
    "notilike",
    "contains",
    "startswith",
    "endswith",
];

/// Neutralise LIKE wildcards the user typed literally, for the ops where we
/// build the pattern ourselves. Paired with `ESCAPE '\'` in the SQL.
fn escape_like(v: &str) -> String {
    v.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// Build a paginated SELECT. `schema` is the trusted column list — every
/// identifier in `filters`/`sort` must appear in it (injection boundary).
pub fn build_select(
    table: &str,
    schema: &[ColumnInfo],
    filters: &[Filter],
    sort: Option<&Sort>,
    limit: i64,
    offset: i64,
    is_pg: bool,
) -> Result<(String, Vec<Bind>), String> {
    let col_type = |name: &str| -> Result<&str, String> {
        schema
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.data_type.as_str())
            .ok_or_else(|| format!("unknown column: {name}"))
    };
    let mut sql = format!("SELECT * FROM {}", quote_ident(table));
    let mut binds: Vec<Bind> = vec![];
    let mut n = 0usize;
    let mut placeholder = || {
        n += 1;
        if is_pg {
            format!("${n}")
        } else {
            "?".to_string()
        }
    };
    let mut where_parts = vec![];
    for f in filters {
        let dt = col_type(&f.column)?;
        let sql_op = OPS
            .iter()
            .find(|(k, _)| *k == f.op)
            .map(|(_, v)| *v)
            .ok_or_else(|| format!("unknown op: {}", f.op))?;
        if f.op == "null" || f.op == "notnull" {
            where_parts.push(format!("{} {}", quote_ident(&f.column), sql_op));
        } else if f.op == "in" || f.op == "notin" {
            let items: Vec<&str> = f
                .value
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            if items.is_empty() {
                return Err(format!("{} needs a comma-separated list of values", f.op));
            }
            let holes: Vec<String> = items.iter().map(|_| placeholder()).collect();
            where_parts.push(format!(
                "{} {} ({})",
                quote_ident(&f.column),
                sql_op,
                holes.join(", ")
            ));
            binds.extend(items.iter().map(|v| Bind::for_column(dt, v)));
        } else if PATTERN_OPS.contains(&f.op.as_str()) {
            // SQLite has no ILIKE; its LIKE is already case-insensitive for ASCII
            let sql_op = if is_pg { sql_op.to_string() } else { sql_op.replace("ILIKE", "LIKE") };
            let (value, escape) = match f.op.as_str() {
                "contains" => (format!("%{}%", escape_like(&f.value)), true),
                "startswith" => (format!("{}%", escape_like(&f.value)), true),
                "endswith" => (format!("%{}", escape_like(&f.value)), true),
                // raw LIKE/ILIKE: the user's % and _ are the whole point, keep them
                _ => (f.value.clone(), false),
            };
            where_parts.push(format!(
                "CAST({} AS TEXT) {} {}{}",
                quote_ident(&f.column),
                sql_op,
                placeholder(),
                if escape { r" ESCAPE '\'" } else { "" }
            ));
            binds.push(Bind::Text(value));
        } else {
            where_parts.push(format!("{} {} {}", quote_ident(&f.column), sql_op, placeholder()));
            binds.push(Bind::for_column(dt, &f.value));
        }
    }
    if !where_parts.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&where_parts.join(" AND "));
    }
    if let Some(s) = sort {
        col_type(&s.column)?;
        sql.push_str(&format!(
            " ORDER BY {} {}",
            quote_ident(&s.column),
            if s.desc { "DESC" } else { "ASC" }
        ));
    }
    sql.push_str(&format!(
        " LIMIT {} OFFSET {}",
        limit.clamp(1, 10_000),
        offset.max(0)
    ));
    Ok((sql, binds))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> Vec<ColumnInfo> {
        vec![
            ColumnInfo {
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                is_pk: true,
            },
            ColumnInfo {
                name: "name".into(),
                data_type: "text".into(),
                nullable: true,
                is_pk: false,
            },
        ]
    }

    #[test]
    fn quotes_identifiers() {
        assert_eq!(quote_ident(r#"we"ird"#), r#""we""ird""#);
    }

    #[test]
    fn rejects_unknown_column() {
        let f = [Filter {
            column: "name; DROP TABLE x".into(),
            op: "eq".into(),
            value: "a".into(),
        }];
        assert!(build_select("t", &schema(), &f, None, 100, 0, true).is_err());
    }

    #[test]
    fn rejects_unknown_op() {
        let f = [Filter {
            column: "name".into(),
            op: "= 1 OR 1=1 --".into(),
            value: "a".into(),
        }];
        assert!(build_select("t", &schema(), &f, None, 100, 0, true).is_err());
    }

    #[test]
    fn builds_pg_select() {
        let f = [
            Filter {
                column: "id".into(),
                op: "gte".into(),
                value: "5".into(),
            },
            Filter {
                column: "name".into(),
                op: "like".into(),
                value: "%bob%".into(),
            },
        ];
        let sort = Sort {
            column: "id".into(),
            desc: true,
        };
        let (sql, binds) = build_select("users", &schema(), &f, Some(&sort), 50, 100, true).unwrap();
        assert_eq!(
            sql,
            r#"SELECT * FROM "users" WHERE "id" >= $1 AND CAST("name" AS TEXT) LIKE $2 ORDER BY "id" DESC LIMIT 50 OFFSET 100"#
        );
        assert!(matches!(binds[0], Bind::Int(5)));
        assert!(matches!(binds[1], Bind::Text(_)));
    }

    #[test]
    fn ilike_downgrades_to_like_on_sqlite() {
        let f = [Filter {
            column: "name".into(),
            op: "ilike".into(),
            value: "%bob%".into(),
        }];
        let (pg, _) = build_select("t", &schema(), &f, None, 10, 0, true).unwrap();
        assert!(pg.contains(r#"CAST("name" AS TEXT) ILIKE $1"#), "{pg}");
        let (lite, _) = build_select("t", &schema(), &f, None, 10, 0, false).unwrap();
        assert!(lite.contains(r#"CAST("name" AS TEXT) LIKE ?"#), "{lite}");
        assert!(!lite.contains("ILIKE"), "{lite}");
    }

    #[test]
    fn contains_wraps_and_escapes_wildcards() {
        let f = [Filter {
            column: "name".into(),
            op: "contains".into(),
            value: "50%_off".into(),
        }];
        let (sql, binds) = build_select("t", &schema(), &f, None, 10, 0, true).unwrap();
        assert!(sql.contains(r"LIKE $1 ESCAPE '\'"), "{sql}");
        match &binds[0] {
            Bind::Text(v) => assert_eq!(v, r"%50\%\_off%"),
            b => panic!("expected text bind, got {b:?}"),
        }
    }

    #[test]
    fn in_expands_to_one_placeholder_per_value() {
        let f = [Filter {
            column: "id".into(),
            op: "in".into(),
            value: "1, 2,3".into(),
        }];
        let (sql, binds) = build_select("t", &schema(), &f, None, 10, 0, true).unwrap();
        assert!(sql.contains(r#""id" IN ($1, $2, $3)"#), "{sql}");
        assert_eq!(binds.len(), 3);
        assert!(matches!(binds[2], Bind::Int(3)));
    }

    #[test]
    fn in_rejects_empty_list() {
        let f = [Filter {
            column: "id".into(),
            op: "in".into(),
            value: " , ".into(),
        }];
        assert!(build_select("t", &schema(), &f, None, 10, 0, true).is_err());
    }
}
