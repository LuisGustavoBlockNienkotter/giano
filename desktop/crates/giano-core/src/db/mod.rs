//! The ONLY place with SQL. `Database` hides the two drivers (a local SQLite file through
//! rusqlite, or Turso through Hrana over HTTP) behind four methods: `execute`, `query`,
//! `query_one` and `transaction`.
//!
//! Every write goes through [`Database::transaction`], which opens `BEGIN IMMEDIATE` so that
//! a whole change (e.g. a new revision) is atomic and serialized against other processes
//! (the TUI, the CLI and this app may run at the same time, on this or other computers).
//!
//! Parameters are positional (`?`, or `?N` to reuse one); both drivers accept them.

pub mod items;
pub mod migrations;
pub mod notes;
pub mod projects;
pub mod remote;
pub mod repo_checks;
pub mod search;
pub mod settings;
pub mod transfer;

use std::cell::Cell;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rusqlite::types::{ToSqlOutput, ValueRef};

use crate::config::is_remote_url;
use crate::error::Result;
use remote::RemoteConnection;

/// A SQL value (the same five types on both drivers).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Integer(v)
    }
}
impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::Integer(v.into())
    }
}
impl From<usize> for Value {
    fn from(v: usize) -> Self {
        Value::Integer(v as i64)
    }
}
impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Integer(v.into())
    }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Real(v)
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Text(v.to_string())
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Text(v)
    }
}
impl From<&String> for Value {
    fn from(v: &String) -> Self {
        Value::Text(v.clone())
    }
}
impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(v: Option<T>) -> Self {
        v.map_or(Value::Null, Into::into)
    }
}

impl rusqlite::ToSql for Value {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Borrowed(match self {
            Value::Null => ValueRef::Null,
            Value::Integer(i) => ValueRef::Integer(*i),
            Value::Real(f) => ValueRef::Real(*f),
            Value::Text(s) => ValueRef::Text(s.as_bytes()),
            Value::Blob(b) => ValueRef::Blob(b),
        }))
    }
}

impl From<ValueRef<'_>> for Value {
    fn from(v: ValueRef<'_>) -> Self {
        match v {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(i) => Value::Integer(i),
            ValueRef::Real(f) => Value::Real(f),
            ValueRef::Text(t) => Value::Text(String::from_utf8_lossy(t).into_owned()),
            ValueRef::Blob(b) => Value::Blob(b.to_vec()),
        }
    }
}

/// `params![a, b, c]` → `&[Value]`.
#[macro_export]
macro_rules! params {
    () => { &[] as &[$crate::db::Value] };
    ($($v:expr),+ $(,)?) => { &[$($crate::db::Value::from($v)),+] as &[$crate::db::Value] };
}

/// Conversion of a column value into a Rust type.
pub trait FromValue: Sized {
    fn from_value(v: &Value) -> Option<Self>;
}

impl FromValue for i64 {
    fn from_value(v: &Value) -> Option<Self> {
        match v {
            Value::Integer(i) => Some(*i),
            Value::Real(f) => Some(*f as i64),
            Value::Text(s) => s.parse().ok(),
            _ => None,
        }
    }
}
impl FromValue for bool {
    fn from_value(v: &Value) -> Option<Self> {
        i64::from_value(v).map(|i| i != 0)
    }
}
impl FromValue for String {
    fn from_value(v: &Value) -> Option<Self> {
        match v {
            Value::Text(s) => Some(s.clone()),
            Value::Integer(i) => Some(i.to_string()),
            Value::Real(f) => Some(f.to_string()),
            _ => None,
        }
    }
}
impl FromValue for Value {
    fn from_value(v: &Value) -> Option<Self> {
        Some(v.clone())
    }
}
impl<T: FromValue> FromValue for Option<T> {
    fn from_value(v: &Value) -> Option<Self> {
        match v {
            Value::Null => Some(None),
            other => T::from_value(other).map(Some),
        }
    }
}

/// A row with access by column name or by position.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    columns: Arc<Vec<String>>,
    values: Vec<Value>,
}

impl Row {
    pub fn new(columns: Arc<Vec<String>>, values: Vec<Value>) -> Self {
        Row { columns, values }
    }

    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    pub fn values(&self) -> &[Value] {
        &self.values
    }

    pub fn into_values(self) -> Vec<Value> {
        self.values
    }

    /// Column by name. Panics on a column the query does not select (a programming error).
    pub fn get<T: FromValue>(&self, name: &str) -> T {
        let idx = self
            .columns
            .iter()
            .position(|c| c == name)
            .unwrap_or_else(|| panic!("coluna inexistente na consulta: {name}"));
        self.at(idx)
    }

    /// Column by position.
    pub fn at<T: FromValue>(&self, idx: usize) -> T {
        let v = &self.values[idx];
        T::from_value(v).unwrap_or_else(|| {
            panic!("tipo inesperado na coluna {}: {v:?}", self.columns.get(idx).map_or("?", |s| s))
        })
    }
}

/// Result of a statement that does not return rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exec {
    pub rows_affected: u64,
    pub last_insert_rowid: i64,
}

enum Backend {
    Local(rusqlite::Connection),
    Remote(RemoteConnection),
}

/// A local SQLite file (or `:memory:`) or a Turso/libSQL URL (`libsql://...`, with a token).
pub struct Database {
    backend: Backend,
    location: String,
    depth: Cell<u32>,
}

impl Database {
    /// Open (and migrate) a database.
    pub fn open(location: &str, auth_token: Option<&str>) -> Result<Database> {
        let backend = if is_remote_url(location) {
            Backend::Remote(RemoteConnection::new(location, auth_token.unwrap_or(""))?)
        } else {
            if location != ":memory:" {
                if let Some(parent) = Path::new(location).parent() {
                    if !parent.as_os_str().is_empty() {
                        fs::create_dir_all(parent)?;
                    }
                }
            }
            let conn = rusqlite::Connection::open(location)?;
            conn.busy_timeout(Duration::from_millis(5000))?;
            Backend::Local(conn)
        };
        let db = Database { backend, location: location.to_string(), depth: Cell::new(0) };
        db.execute("PRAGMA foreign_keys = ON", params![])?;
        if !db.is_remote() && location != ":memory:" {
            db.query("PRAGMA journal_mode = WAL", params![])?;
        }
        migrations::migrate(&db)?;
        Ok(db)
    }

    pub fn open_local(path: impl AsRef<Path>) -> Result<Database> {
        Database::open(&path.as_ref().to_string_lossy(), None)
    }

    /// File path or Turso URL (never the token).
    pub fn location(&self) -> &str {
        &self.location
    }

    pub fn is_remote(&self) -> bool {
        matches!(self.backend, Backend::Remote(_))
    }

    pub fn execute(&self, sql: &str, params: &[Value]) -> Result<Exec> {
        match &self.backend {
            Backend::Local(conn) => {
                let mut stmt = conn.prepare_cached(sql)?;
                // query() instead of execute(): PRAGMAs may return a row.
                let mut rows = stmt.query(rusqlite::params_from_iter(params.iter()))?;
                while rows.next()?.is_some() {}
                Ok(Exec { rows_affected: conn.changes(), last_insert_rowid: conn.last_insert_rowid() })
            }
            Backend::Remote(r) => {
                let res = r.execute(sql, params)?;
                Ok(Exec {
                    rows_affected: res.affected_row_count,
                    last_insert_rowid: res.last_insert_rowid.unwrap_or(0),
                })
            }
        }
    }

    pub fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>> {
        match &self.backend {
            Backend::Local(conn) => {
                let mut stmt = conn.prepare_cached(sql)?;
                let columns: Arc<Vec<String>> =
                    Arc::new(stmt.column_names().iter().map(|c| c.to_string()).collect());
                let n = columns.len();
                let mut rows = stmt.query(rusqlite::params_from_iter(params.iter()))?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    let mut values = Vec::with_capacity(n);
                    for i in 0..n {
                        values.push(Value::from(row.get_ref(i)?));
                    }
                    out.push(Row::new(columns.clone(), values));
                }
                Ok(out)
            }
            Backend::Remote(r) => {
                let res = r.execute(sql, params)?;
                let columns = Arc::new(res.columns);
                Ok(res.rows.into_iter().map(|values| Row::new(columns.clone(), values)).collect())
            }
        }
    }

    pub fn query_one(&self, sql: &str, params: &[Value]) -> Result<Option<Row>> {
        Ok(self.query(sql, params)?.into_iter().next())
    }

    /// Several statements separated by `;` (migrations).
    pub fn execute_script(&self, sql: &str) -> Result<()> {
        match &self.backend {
            Backend::Local(conn) => Ok(conn.execute_batch(sql)?),
            Backend::Remote(r) => r.execute_script(sql),
        }
    }

    /// Keep one connection state for the whole block. A no-op on a local file; on Turso it
    /// keeps the same Hrana stream (a transaction and `PRAGMA`s only live in their stream).
    pub fn session<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<T> {
        match &self.backend {
            Backend::Local(_) => f(),
            Backend::Remote(r) => {
                r.pin();
                let result = f();
                let closed = r.unpin();
                match (result, closed) {
                    (Ok(v), Ok(())) => Ok(v),
                    (Ok(_), Err(e)) => Err(e),
                    (Err(e), _) => Err(e),
                }
            }
        }
    }

    /// Run a block in a single transaction. Nested calls join the outer one.
    pub fn transaction<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<T> {
        if self.depth.get() > 0 {
            self.depth.set(self.depth.get() + 1);
            let result = f();
            self.depth.set(self.depth.get() - 1);
            return result;
        }
        self.session(|| {
            self.execute("BEGIN IMMEDIATE", params![])?;
            self.depth.set(1);
            let result = f();
            self.depth.set(0);
            match result {
                Ok(v) => match self.execute("COMMIT", params![]) {
                    Ok(_) => Ok(v),
                    Err(e) => {
                        self.rollback();
                        Err(e)
                    }
                },
                Err(e) => {
                    self.rollback();
                    Err(e)
                }
            }
        })
    }

    pub fn in_transaction(&self) -> bool {
        self.depth.get() > 0
    }

    fn rollback(&self) {
        // Never hide the original error (e.g. the network dropped and ROLLBACK fails too).
        let _ = self.execute("ROLLBACK", params![]);
    }
}
