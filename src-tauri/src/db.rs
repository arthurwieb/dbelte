use crate::meta::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::mysql::{MySqlConnectOptions, MySqlPool, MySqlRow};
use sqlx::postgres::{PgConnectOptions, PgPool, PgRow};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqliteRow};
use sqlx::{Column, Row, TypeInfo};

pub enum DbPool {
    Pg(PgPool),
    Sqlite(SqlitePool),
    My(MySqlPool),
    Mssql(crate::mssql::Pool),
}

impl DbPool {
    pub fn dialect(&self) -> Dialect {
        match self {
            DbPool::Pg(_) => Dialect::Pg,
            DbPool::Sqlite(_) => Dialect::Sqlite,
            DbPool::My(_) => Dialect::MySql,
            DbPool::Mssql(_) => Dialect::Mssql,
        }
    }
}

/// Everything the SQL builders need to know about an engine. Separate from
/// `DbPool` because the builders are pure and testable without a live server,
/// and because the same dialect can be reached through more than one driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Pg,
    Sqlite,
    MySql,
    Mssql,
}

impl Dialect {
    /// Bind placeholder for the nth parameter, counting from 1.
    pub fn placeholder(&self, n: usize) -> String {
        match self {
            Dialect::Pg => format!("${n}"),
            Dialect::Sqlite | Dialect::MySql => "?".to_string(),
            Dialect::Mssql => format!("@P{n}"),
        }
    }

    /// Quote an identifier so it is safe to interpolate.
    pub fn quote_ident(&self, name: &str) -> String {
        match self {
            Dialect::Pg | Dialect::Sqlite => format!("\"{}\"", name.replace('"', "\"\"")),
            Dialect::MySql => format!("`{}`", name.replace('`', "``")),
            Dialect::Mssql => format!("[{}]", name.replace(']', "]]")),
        }
    }

    /// Postgres types a bare `$n` as text, so binding a string against a uuid,
    /// enum, date or json column fails with "operator does not exist: uuid =
    /// text". Cast the placeholder to the column's declared type instead. The
    /// other engines infer from the column and need nothing.
    pub fn cast_ph(&self, placeholder: &str, data_type: &str) -> String {
        match self {
            Dialect::Pg => format!("CAST({placeholder} AS {data_type})"),
            _ => placeholder.to_string(),
        }
    }

    /// Cast target for comparing any column as text, which is how the pattern
    /// ops work on numeric and date columns.
    pub fn text_type(&self) -> &'static str {
        match self {
            Dialect::Pg | Dialect::Sqlite => "TEXT",
            Dialect::MySql => "CHAR",
            Dialect::Mssql => "NVARCHAR(MAX)",
        }
    }

    /// ILIKE only exists on Postgres. Everywhere else LIKE is already
    /// case-insensitive — SQLite for ASCII, MySQL and SQL Server by collation —
    /// so downgrading is the closest honest match.
    pub fn ilike(&self, op: &str) -> String {
        match self {
            Dialect::Pg => op.to_string(),
            _ => op.replace("ILIKE", "LIKE"),
        }
    }

    /// SQL Server spells it `ALTER TABLE t ADD col type`, with no COLUMN.
    pub fn add_column_kw(&self) -> &'static str {
        match self {
            Dialect::Mssql => "ADD",
            _ => "ADD COLUMN",
        }
    }

    /// The row-window clause. SQL Server's OFFSET…FETCH is only legal after an
    /// ORDER BY, so supply a no-op one when the caller had no sort.
    pub fn paginate(&self, limit: i64, offset: i64, has_sort: bool) -> String {
        match self {
            Dialect::Mssql => {
                let order = if has_sort { "" } else { " ORDER BY (SELECT NULL)" };
                format!("{order} OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY")
            }
            _ => format!(" LIMIT {limit} OFFSET {offset}"),
        }
    }
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

/// A single-column foreign key, for "jump to the referenced row". Composite
/// keys are left out — one equality filter can't express them.
#[derive(Debug, Serialize)]
pub struct ForeignKey {
    pub column: String,
    /// where the referenced table lives — a foreign key can cross schemas
    pub ref_schema: Option<String>,
    pub ref_table: String,
    pub ref_column: String,
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
                // an unreachable host otherwise hangs the UI until sqlx gives up
                .acquire_timeout(std::time::Duration::from_secs(10))
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
                .acquire_timeout(std::time::Duration::from_secs(10))
                .connect_with(opts)
                .await
                .map_err(|e| e.to_string())?;
            Ok(DbPool::Sqlite(pool))
        }
        "mysql" => {
            let mut opts = MySqlConnectOptions::new()
                .host(conn.host.as_deref().unwrap_or("localhost"))
                .port(conn.port.unwrap_or(3306) as u16)
                .database(&conn.database)
                .username(conn.username.as_deref().unwrap_or("root"));
            if let Some(pw) = password.as_deref() {
                opts = opts.password(pw);
            }
            let pool = sqlx::pool::PoolOptions::new()
                .max_connections(4)
                .acquire_timeout(std::time::Duration::from_secs(10))
                .connect_with(opts)
                .await
                .map_err(|e| e.to_string())?;
            Ok(DbPool::My(pool))
        }
        "mssql" => {
            let pool = crate::mssql::connect(
                conn.host.as_deref().unwrap_or("localhost"),
                conn.port.unwrap_or(1433) as u16,
                &conn.database,
                conn.username.as_deref().unwrap_or("sa"),
                password.as_deref().unwrap_or(""),
            )
            .await?;
            Ok(DbPool::Mssql(pool))
        }
        e => Err(format!("unknown engine: {e}")),
    }
}

/// A table, plus the schema it lives in. Postgres has many schemas and a table
/// name alone is ambiguous between them; SQLite has none, so `schema` is None
/// there. Kept as a pair rather than a "schema.table" string because a table
/// name is allowed to contain a dot, and splitting one back apart guesses wrong.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableRef {
    pub schema: Option<String>,
    pub name: String,
}

impl std::fmt::Display for TableRef {
    /// `reporting.orders`, unquoted — for error messages, never for SQL
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.schema {
            Some(s) => write!(f, "{s}.{}", self.name),
            None => write!(f, "{}", self.name),
        }
    }
}

impl TableRef {
    /// `"public"."orders"` — both parts quoted, safe to interpolate
    pub fn quoted(&self, d: Dialect) -> String {
        match &self.schema {
            Some(s) => format!("{}.{}", d.quote_ident(s), d.quote_ident(&self.name)),
            None => d.quote_ident(&self.name),
        }
    }
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
    [
        "select", "with", "pragma", "explain", "show", "values", "describe", "desc",
    ]
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

/// Run `sql` against anything sqlx will execute against — a pool or a single
/// pinned connection — decoding fetched rows with `$value`. Statement shape is
/// identical per engine; only the value decoder differs.
macro_rules! run {
    ($executor:expr, $sql:expr, $binds:expr, $value:path) => {{
        if is_fetch($sql) {
            let rows = bind_all!(sqlx::query($sql), $binds)
                .fetch_all($executor)
                .await
                .map_err(|e| e.to_string())?;
            Ok(rows_to_result(&rows, $value))
        } else {
            let affected = bind_all!(sqlx::query($sql), $binds)
                .execute($executor)
                .await
                .map_err(|e| e.to_string())?
                .rows_affected();
            Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                rows_affected: affected,
            })
        }
    }};
}

pub async fn execute(pool: &DbPool, sql: &str, binds: Vec<Bind>) -> Result<QueryResult, String> {
    match pool {
        DbPool::Pg(p) => run!(p, sql, binds, pg_value),
        DbPool::Sqlite(p) => run!(p, sql, binds, sqlite_value),
        DbPool::My(p) => run!(p, sql, binds, mysql_value),
        // tiberius shares none of sqlx's traits, so this arm goes its own way
        DbPool::Mssql(p) => crate::mssql::execute(p, sql, binds).await,
    }
}

/// Run a query on one specific connection. `execute` takes a pool and may land
/// on any connection in it; cancellation needs the query pinned to the session
/// whose id we handed to `pg_cancel_backend` or `KILL QUERY`.
pub async fn execute_pg(
    conn: &mut sqlx::PgConnection,
    sql: &str,
    binds: Vec<Bind>,
) -> Result<QueryResult, String> {
    run!(&mut *conn, sql, binds, pg_value)
}

pub async fn execute_mysql(
    conn: &mut sqlx::MySqlConnection,
    sql: &str,
    binds: Vec<Bind>,
) -> Result<QueryResult, String> {
    run!(&mut *conn, sql, binds, mysql_value)
}

fn rows_to_result<R: Row>(rows: &[R], value: fn(&R, usize) -> Value) -> QueryResult {
    let columns = rows
        .first()
        .map(|r| r.columns().iter().map(|c| c.name().to_string()).collect())
        .unwrap_or_default();
    let data = rows
        .iter()
        .map(|row| (0..row.columns().len()).map(|i| value(row, i)).collect())
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

/// MySQL reports a concrete type per column like Postgres does, but with its
/// own names and a signed/unsigned split that changes which Rust integer fits.
fn mysql_value(row: &MySqlRow, i: usize) -> Value {
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
        // sqlx surfaces tinyint(1) as BOOLEAN, which is the only bool MySQL has
        "BOOLEAN" => get!(bool),
        "TINYINT" => get!(i8),
        "SMALLINT" => get!(i16),
        "INT" | "MEDIUMINT" => get!(i32),
        "BIGINT" => get!(i64),
        "TINYINT UNSIGNED" => get!(u8),
        "SMALLINT UNSIGNED" => get!(u16),
        "INT UNSIGNED" | "MEDIUMINT UNSIGNED" => get!(u32),
        "BIGINT UNSIGNED" => get!(u64),
        "FLOAT" => get!(f32),
        "DOUBLE" => get!(f64),
        "DECIMAL" => {
            if let Ok(v) = row.try_get::<Option<rust_decimal::Decimal>, _>(i) {
                return v.map(|d| json!(d.to_string())).unwrap_or(Value::Null);
            }
        }
        "JSON" => get!(Value),
        "DATETIME" | "TIMESTAMP" => {
            if let Ok(v) = row.try_get::<Option<chrono::NaiveDateTime>, _>(i) {
                return v.map(|d| json!(d.to_string())).unwrap_or(Value::Null);
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
        "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" | "BINARY" | "VARBINARY" => {
            if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(i) {
                return v
                    .map(|b| json!(format!("\\x{}", hex_str(&b))))
                    .unwrap_or(Value::Null);
            }
        }
        _ => {}
    }
    // fallback: char/varchar/text/enum/set and anything unknown
    match row.try_get::<Option<String>, _>(i) {
        Ok(Some(v)) => json!(v),
        Ok(None) => Value::Null,
        Err(_) => json!("<unsupported>"),
    }
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

/// The LIKE escape character. Not a backslash: MySQL processes backslashes
/// inside string literals, so `ESCAPE '\'` is an unterminated string there and
/// `ESCAPE '\\'` breaks in turn under NO_BACKSLASH_ESCAPES. `!` is ordinary in
/// every engine's literals and in LIKE patterns, so one spelling works for all.
const LIKE_ESCAPE: char = '!';

/// Neutralise LIKE wildcards the user typed literally, for the ops where we
/// build the pattern ourselves. Paired with `ESCAPE '!'` in the SQL.
fn escape_like(v: &str) -> String {
    v.replace(LIKE_ESCAPE, "!!")
        .replace('%', "!%")
        .replace('_', "!_")
}

/// The WHERE clause shared by the row query and the count query — the one place
/// user input becomes SQL, so identifiers are checked against the live schema
/// and values are always bound.
fn build_where(
    schema: &[ColumnInfo],
    filters: &[Filter],
    d: Dialect,
) -> Result<(String, Vec<Bind>), String> {
    let col_type = |name: &str| -> Result<&str, String> {
        schema
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.data_type.as_str())
            .ok_or_else(|| format!("unknown column: {name}"))
    };
    let mut binds: Vec<Bind> = vec![];
    let mut n = 0usize;
    let mut placeholder = || {
        n += 1;
        d.placeholder(n)
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
            where_parts.push(format!("{} {}", d.quote_ident(&f.column), sql_op));
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
            let holes: Vec<String> = items
                .iter()
                .map(|_| d.cast_ph(&placeholder(), dt))
                .collect();
            where_parts.push(format!(
                "{} {} ({})",
                d.quote_ident(&f.column),
                sql_op,
                holes.join(", ")
            ));
            binds.extend(items.iter().map(|v| Bind::for_column(dt, v)));
        } else if PATTERN_OPS.contains(&f.op.as_str()) {
            let sql_op = d.ilike(sql_op);
            let (value, escape) = match f.op.as_str() {
                "contains" => (format!("%{}%", escape_like(&f.value)), true),
                "startswith" => (format!("{}%", escape_like(&f.value)), true),
                "endswith" => (format!("%{}", escape_like(&f.value)), true),
                // raw LIKE/ILIKE: the user's % and _ are the whole point, keep them
                _ => (f.value.clone(), false),
            };
            where_parts.push(format!(
                "CAST({} AS {}) {} {}{}",
                d.quote_ident(&f.column),
                d.text_type(),
                sql_op,
                placeholder(),
                if escape { " ESCAPE '!'" } else { "" }
            ));
            binds.push(Bind::Text(value));
        } else {
            where_parts.push(format!(
                "{} {} {}",
                d.quote_ident(&f.column),
                sql_op,
                d.cast_ph(&placeholder(), dt)
            ));
            binds.push(Bind::for_column(dt, &f.value));
        }
    }
    if where_parts.is_empty() {
        return Ok((String::new(), binds));
    }
    Ok((format!(" WHERE {}", where_parts.join(" AND ")), binds))
}

/// `SELECT count(*)` under the same filters, for the pager's total.
pub fn build_count(
    table: &TableRef,
    schema: &[ColumnInfo],
    filters: &[Filter],
    d: Dialect,
) -> Result<(String, Vec<Bind>), String> {
    let (where_sql, binds) = build_where(schema, filters, d)?;
    Ok((
        format!("SELECT count(*) FROM {}{}", table.quoted(d), where_sql),
        binds,
    ))
}

/// Build a SELECT. `schema` is the trusted column list — every identifier in
/// `filters`/`sort` must appear in it (injection boundary). `limit <= 0` means
/// no LIMIT/OFFSET at all, which is how export streams a whole table.
pub fn build_select(
    table: &TableRef,
    schema: &[ColumnInfo],
    filters: &[Filter],
    sort: Option<&Sort>,
    limit: i64,
    offset: i64,
    d: Dialect,
) -> Result<(String, Vec<Bind>), String> {
    let col_type = |name: &str| -> Result<&str, String> {
        schema
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.data_type.as_str())
            .ok_or_else(|| format!("unknown column: {name}"))
    };
    let mut sql = format!("SELECT * FROM {}", table.quoted(d));
    let (where_sql, binds) = build_where(schema, filters, d)?;
    sql.push_str(&where_sql);
    if let Some(s) = sort {
        col_type(&s.column)?;
        sql.push_str(&format!(
            " ORDER BY {} {}",
            d.quote_ident(&s.column),
            if s.desc { "DESC" } else { "ASC" }
        ));
    }
    if limit > 0 {
        sql.push_str(&d.paginate(limit.min(10_000), offset.max(0), sort.is_some()));
    }
    Ok((sql, binds))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// unqualified table, the SQLite shape
    fn t(name: &str) -> TableRef {
        TableRef { schema: None, name: name.into() }
    }

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
        assert_eq!(Dialect::Pg.quote_ident(r#"we"ird"#), r#""we""ird""#);
    }

    #[test]
    fn rejects_unknown_column() {
        let f = [Filter {
            column: "name; DROP TABLE x".into(),
            op: "eq".into(),
            value: "a".into(),
        }];
        assert!(build_select(&t("t"), &schema(), &f, None, 100, 0, Dialect::Pg).is_err());
    }

    #[test]
    fn rejects_unknown_op() {
        let f = [Filter {
            column: "name".into(),
            op: "= 1 OR 1=1 --".into(),
            value: "a".into(),
        }];
        assert!(build_select(&t("t"), &schema(), &f, None, 100, 0, Dialect::Pg).is_err());
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
        let (sql, binds) =
            build_select(&t("users"), &schema(), &f, Some(&sort), 50, 100, Dialect::Pg).unwrap();
        assert_eq!(
            sql,
            r#"SELECT * FROM "users" WHERE "id" >= CAST($1 AS integer) AND CAST("name" AS TEXT) LIKE $2 ORDER BY "id" DESC LIMIT 50 OFFSET 100"#
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
        let (pg, _) = build_select(&t("t"), &schema(), &f, None, 10, 0, Dialect::Pg).unwrap();
        assert!(pg.contains(r#"CAST("name" AS TEXT) ILIKE $1"#), "{pg}");
        let (lite, _) = build_select(&t("t"), &schema(), &f, None, 10, 0, Dialect::Sqlite).unwrap();
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
        let (sql, binds) = build_select(&t("t"), &schema(), &f, None, 10, 0, Dialect::Pg).unwrap();
        assert!(sql.contains("LIKE $1 ESCAPE '!'"), "{sql}");
        match &binds[0] {
            Bind::Text(v) => assert_eq!(v, "%50!%!_off%"),
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
        let (sql, binds) = build_select(&t("t"), &schema(), &f, None, 10, 0, Dialect::Pg).unwrap();
        assert!(
            sql.contains(
                r#""id" IN (CAST($1 AS integer), CAST($2 AS integer), CAST($3 AS integer))"#
            ),
            "{sql}"
        );
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
        assert!(build_select(&t("t"), &schema(), &f, None, 10, 0, Dialect::Pg).is_err());
    }

    #[test]
    fn count_reuses_the_filters_and_skips_limit() {
        let f = [Filter {
            column: "name".into(),
            op: "eq".into(),
            value: "bob".into(),
        }];
        let (sql, binds) = build_count(&t("t"), &schema(), &f, Dialect::Pg).unwrap();
        assert_eq!(
            sql,
            r#"SELECT count(*) FROM "t" WHERE "name" = CAST($1 AS text)"#
        );
        assert_eq!(binds.len(), 1);
        let (bare, _) = build_count(&t("t"), &schema(), &[], Dialect::Pg).unwrap();
        assert_eq!(bare, r#"SELECT count(*) FROM "t""#);
    }

    #[test]
    fn zero_limit_means_no_limit_clause() {
        let (sql, _) = build_select(&t("t"), &schema(), &[], None, 0, 0, Dialect::Pg).unwrap();
        assert!(!sql.contains("LIMIT"), "{sql}");
    }

    #[test]
    fn qualified_table_quotes_both_parts() {
        let table = TableRef { schema: Some("reporting".into()), name: "orders".into() };
        let (sql, _) = build_select(&table, &schema(), &[], None, 10, 0, Dialect::Pg).unwrap();
        assert!(sql.starts_with(r#"SELECT * FROM "reporting"."orders""#), "{sql}");
        let (count, _) = build_count(&table, &schema(), &[], Dialect::Pg).unwrap();
        assert_eq!(count, r#"SELECT count(*) FROM "reporting"."orders""#);
    }

    #[test]
    fn mssql_paginates_with_offset_fetch_and_needs_an_order_by() {
        let sorted = Sort { column: "id".into(), desc: false };
        let (sql, _) =
            build_select(&t("t"), &schema(), &[], Some(&sorted), 50, 100, Dialect::Mssql).unwrap();
        assert!(sql.ends_with("OFFSET 100 ROWS FETCH NEXT 50 ROWS ONLY"), "{sql}");
        assert!(!sql.contains("SELECT NULL"), "{sql}");
        // no sort: OFFSET…FETCH is a syntax error without ORDER BY, so stub one
        let (bare, _) = build_select(&t("t"), &schema(), &[], None, 50, 0, Dialect::Mssql).unwrap();
        assert!(
            bare.contains("ORDER BY (SELECT NULL) OFFSET 0 ROWS FETCH NEXT 50 ROWS ONLY"),
            "{bare}"
        );
    }

    #[test]
    fn quotes_per_dialect() {
        assert_eq!(Dialect::MySql.quote_ident("we`ird"), "`we``ird`");
        assert_eq!(Dialect::Mssql.quote_ident("we]ird"), "[we]]ird]");
    }

    #[test]
    fn placeholders_per_dialect() {
        assert_eq!(Dialect::Pg.placeholder(2), "$2");
        assert_eq!(Dialect::Sqlite.placeholder(2), "?");
        assert_eq!(Dialect::MySql.placeholder(2), "?");
        assert_eq!(Dialect::Mssql.placeholder(2), "@P2");
    }
}
