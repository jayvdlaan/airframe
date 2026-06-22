use airframe_db::connection::{DbConnection, DbPool, SqlExec, SqlParam, SqlRows, SqlValue};
use airframe_db::error::{AirframeDbError, Result};
use rusqlite::Error as SqliteError;
use rusqlite::ErrorCode as SqliteErrorCode;
use rusqlite::{
    params_from_iter, types::Type as SqliteType, types::Value as SqliteValue, Connection,
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::thread;
use std::time::Duration;
use tracing::{debug, error, info, instrument, trace, warn};

fn to_db_err<E: core::fmt::Display>(stage: &'static str, e: E) -> AirframeDbError {
    match stage {
        "open" => AirframeDbError::Connection(e.to_string()),
        "ping" => AirframeDbError::Connection(e.to_string()),
        _ => AirframeDbError::InvalidState,
    }
}

/// A SQLite connection implementing DbConnection and SqlExec.
#[derive(Clone)]
pub struct SqliteConn {
    path: String,
    pragmas: Vec<String>,
}

impl SqliteConn {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            pragmas: vec!["PRAGMA foreign_keys=ON".into()],
        }
    }
    pub fn from_parts(path: String, pragmas: Vec<String>) -> Self {
        Self { path, pragmas }
    }

    fn open(&self) -> Result<Connection> {
        let path = self.path.as_str();
        // If path is file-like (not ":memory:"), ensure parent directory exists
        if path != ":memory:" {
            if let Some(parent) = std::path::Path::new(path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        info!(target = "airframe_sqlite", path = %path, "open database");
        // Retry open if the database is busy/locked.
        let conn = {
            let mut ms: u64 = 5;
            let mut attempt: u32 = 0;
            loop {
                match Connection::open(path) {
                    Ok(c) => break c,
                    Err(e) => {
                        attempt += 1;
                        if is_busy(&e) && attempt <= 5 {
                            trace!(target = "airframe_sqlite", backoff_ms = %ms, "busy retry");
                            if attempt >= 4 {
                                warn!(target = "airframe_sqlite", attempt = %attempt, "open busy threshold");
                            }
                            thread::sleep(Duration::from_millis(ms));
                            ms = (ms * 2).min(200);
                            continue;
                        } else {
                            return Err(to_db_err("open", e));
                        }
                    }
                }
            }
        };
        for p in &self.pragmas {
            // Try to parse simple PRAGMA name/value for logging
            let (name, val) = parse_pragma(p);
            debug!(target = "airframe_sqlite", pragma = %name, value = %val);
            conn.execute(p.as_str(), [])
                .map_err(|_e| AirframeDbError::InvalidState)?;
        }
        Ok(conn)
    }
}

impl DbConnection for SqliteConn {
    fn ping(&self) -> Result<()> {
        let conn = self.open()?;
        conn.query_row("SELECT 1", [], |_row| Ok(()))
            .map_err(|e| to_db_err("ping", e))
    }
}

impl SqlExec for SqliteConn {
    #[instrument(level = "debug", skip(self, params, sql), fields(query_hash = %query_hash(sql)))]
    fn execute(&self, sql: &str, params: &[SqlParam]) -> Result<u64> {
        let values: Vec<SqliteValue> = params
            .iter()
            .map(sqlparam_to_sqlite)
            .collect::<Result<Vec<_>>>()?;
        let conn = self.open()?;
        let h = query_hash(sql);
        let mut ms: u64 = 5;
        let mut attempt: u32 = 0;
        let n = loop {
            match conn.execute(sql, params_from_iter(values.clone())) {
                Ok(n) => break n,
                Err(e) => {
                    if is_busy(&e) && attempt < 5 {
                        attempt += 1;
                        trace!(target = "airframe_sqlite", backoff_ms = %ms, "busy retry");
                        if attempt >= 4 {
                            warn!(target = "airframe_sqlite", attempt = %attempt, "exec busy threshold");
                        }
                        thread::sleep(Duration::from_millis(ms));
                        ms = (ms * 2).min(200);
                        continue;
                    }
                    error!(target = "airframe_sqlite", query_hash = %h, error = ?e, "query failed");
                    return Err(AirframeDbError::InvalidState);
                }
            }
        };
        Ok(n as u64)
    }

    #[instrument(level = "debug", skip(self, params, sql), fields(query_hash = %query_hash(sql)))]
    fn query(&self, sql: &str, params: &[SqlParam]) -> Result<SqlRows> {
        let values: Vec<SqliteValue> = params
            .iter()
            .map(sqlparam_to_sqlite)
            .collect::<Result<Vec<_>>>()?;
        let conn = self.open()?;
        let h = query_hash(sql);
        let mut stmt = {
            let mut ms: u64 = 5;
            let mut attempt: u32 = 0;
            loop {
                match conn.prepare(sql) {
                    Ok(s) => break s,
                    Err(e) => {
                        if is_busy(&e) && attempt < 5 {
                            attempt += 1;
                            trace!(target = "airframe_sqlite", backoff_ms = %ms, "busy retry");
                            if attempt >= 4 {
                                warn!(target = "airframe_sqlite", attempt = %attempt, "prepare busy threshold");
                            }
                            thread::sleep(Duration::from_millis(ms));
                            ms = (ms * 2).min(200);
                            continue;
                        }
                        error!(target = "airframe_sqlite", query_hash = %h, error = ?e, "query failed");
                        return Err(AirframeDbError::InvalidState);
                    }
                }
            }
        };
        let col_count = stmt.column_count();
        let columns = (0..col_count)
            .map(|i| stmt.column_name(i).unwrap_or("").to_string())
            .collect::<Vec<_>>();
        let mut rows = Vec::new();
        let mut mapped = loop {
            match stmt.query(params_from_iter(values.clone())) {
                Ok(m) => break m,
                Err(e) => {
                    if is_busy(&e) {
                        trace!(target = "airframe_sqlite", backoff_ms = 5, "busy retry");
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    error!(target = "airframe_sqlite", query_hash = %h, error = ?e, "query failed");
                    return Err(AirframeDbError::InvalidState);
                }
            }
        };
        while let Some(row) = mapped.next().map_err(|_e| AirframeDbError::InvalidState)? {
            let mut out_row: Vec<SqlValue> = Vec::with_capacity(col_count);
            for i in 0..col_count {
                let vref = row.get_ref(i).map_err(|_e| AirframeDbError::InvalidState)?;
                let v = match vref.data_type() {
                    SqliteType::Null => SqlValue::Null,
                    SqliteType::Integer => {
                        let val = vref.as_i64().map_err(|_e| AirframeDbError::InvalidState)?;
                        SqlValue::I64(val)
                    }
                    SqliteType::Real => {
                        let val = vref.as_f64().map_err(|_e| AirframeDbError::InvalidState)?;
                        SqlValue::F64(val)
                    }
                    SqliteType::Text => {
                        let val = vref.as_str().map_err(|_e| AirframeDbError::InvalidState)?;
                        SqlValue::Str(val.to_string())
                    }
                    SqliteType::Blob => {
                        let val = vref.as_blob().map_err(|_e| AirframeDbError::InvalidState)?;
                        SqlValue::Bytes(val.to_vec())
                    }
                };
                out_row.push(v);
            }
            rows.push(out_row);
        }
        Ok(SqlRows { columns, rows })
    }
}

fn sqlparam_to_sqlite(p: &SqlParam) -> Result<SqliteValue> {
    Ok(match p {
        SqlParam::Null => SqliteValue::Null,
        SqlParam::I64(v) => SqliteValue::Integer(*v),
        SqlParam::U64(v) => {
            let iv = i64::try_from(*v).map_err(|_| AirframeDbError::InvalidState)?;
            SqliteValue::Integer(iv)
        }
        SqlParam::F64(v) => SqliteValue::Real(*v),
        SqlParam::Bool(b) => SqliteValue::Integer(if *b { 1 } else { 0 }),
        SqlParam::Str(s) => SqliteValue::Text((*s).to_string()),
        SqlParam::Bytes(b) => SqliteValue::Blob((*b).to_vec()),
    })
}

fn is_busy(e: &SqliteError) -> bool {
    match e {
        SqliteError::SqliteFailure(err, _) => matches!(
            err.code,
            SqliteErrorCode::DatabaseBusy | SqliteErrorCode::DatabaseLocked
        ),
        _ => false,
    }
}

fn query_hash(sql: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    sql.hash(&mut hasher);
    hasher.finish()
}

fn parse_pragma(s: &str) -> (String, String) {
    // Expect forms like: "PRAGMA name=value" or "PRAGMA name = value"
    let trimmed = s.trim();
    let no_prefix = trimmed.strip_prefix("PRAGMA ").unwrap_or(trimmed);
    if let Some((n, v)) = no_prefix.split_once('=') {
        (n.trim().to_string(), v.trim().to_string())
    } else {
        (no_prefix.trim().to_string(), "".into())
    }
}

/// A simple pool that opens a new rusqlite::Connection per get().
#[derive(Clone)]
pub struct SqlitePool {
    path: String,
    pragmas: Vec<String>,
}

impl SqlitePool {
    pub fn new(path: impl Into<String>) -> Self {
        let path_str = path.into();
        // Ensure directory exists for file paths
        if path_str.as_str() != ":memory:" {
            if let Some(parent) = std::path::Path::new(&path_str).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        Self {
            path: path_str,
            pragmas: vec!["PRAGMA foreign_keys=ON".into()],
        }
    }
    pub fn memory() -> Self {
        Self::new(":memory:")
    }
    /// Provide additional PRAGMA statements to be executed on open.
    pub fn with_pragmas(mut self, pragmas: Vec<String>) -> Self {
        self.pragmas.extend(pragmas);
        self
    }
}

impl DbPool for SqlitePool {
    type Conn = SqliteConn;
    fn get(&self) -> Result<Self::Conn> {
        Ok(SqliteConn::from_parts(
            self.path.clone(),
            self.pragmas.clone(),
        ))
    }
}
