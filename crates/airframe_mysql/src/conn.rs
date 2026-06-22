// Compiled only with feature = "driver" (gated at the `mod conn;` site in lib.rs).

use airframe_db::connection::{DbConnection, DbPool, SqlExec, SqlParam, SqlRows, SqlValue};
use airframe_db::error::{AirframeDbError, Result};
use mysql::{prelude::Queryable, Conn, Opts, Row, Value};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use tracing::{error, info, instrument, warn};

fn to_db_err<E: core::fmt::Display>(stage: &'static str, e: E) -> AirframeDbError {
    match stage {
        "open" => AirframeDbError::Connection(e.to_string()),
        "ping" => AirframeDbError::Connection(e.to_string()),
        "exec" => AirframeDbError::Other(1001),
        _ => AirframeDbError::InvalidState,
    }
}

#[derive(Clone)]
pub struct MySqlConn {
    url: String,
    /// Optional database to use (schema). If present but not in URL, we'll set it via "USE db".
    default_db: Option<String>,
}

impl MySqlConn {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            default_db: None,
        }
    }
    pub fn with_db(url: impl Into<String>, db: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            default_db: Some(db.into()),
        }
    }

    fn open(&self) -> Result<Conn> {
        // Allow URLs without db and set it later via USE if provided.
        let opts = Opts::from_url(&self.url).map_err(|e| to_db_err("open", e))?;
        // Busy/connect retry: up to 3 attempts (initial + 2 retries) with a fixed
        // 200ms backoff between them, retrying on any connect/USE error. Delegates
        // the attempt loop to the shared core primitive.
        let policy = airframe_core::retry::RetryPolicy {
            max_retries: 2,
            backoff: airframe_core::retry::Backoff::Fixed(Duration::from_millis(200)),
            jitter_frac: 0.0,
        };
        airframe_core::retry::retry(
            policy,
            |attempt| {
                if attempt > 0 {
                    warn!(target = "airframe_mysql", attempt = %attempt, "reconnect loop");
                }
                let mut conn = Conn::new(opts.clone()).map_err(|e| to_db_err("open", e))?;
                if let Some(db) = &self.default_db {
                    if !db.is_empty() {
                        // Escape backticks in the identifier (MySQL doubles them)
                        // so a crafted database name cannot break out of the quoting.
                        let db_ident = db.replace('`', "``");
                        conn.query_drop(format!("USE `{db_ident}`"))
                            .map_err(|e| to_db_err("open", e))?;
                    }
                }
                Ok(conn)
            },
            |_e| true,
        )
    }
}

impl DbConnection for MySqlConn {
    fn ping(&self) -> Result<()> {
        let mut conn = self.open()?;
        conn.ping().map_err(|e| to_db_err("ping", e))
    }
}

impl SqlExec for MySqlConn {
    #[instrument(level = "debug", skip(self, params, sql), fields(query_hash = %query_hash(sql)))]
    fn execute(&self, sql: &str, params: &[SqlParam]) -> Result<u64> {
        let mut conn = self.open()?;
        let params = params
            .iter()
            .map(sqlparam_to_mysql)
            .collect::<Result<Vec<Value>>>()?;
        let h = query_hash(sql);
        conn.exec_drop(sql, params).map_err(|e| {
            error!(target = "airframe_mysql", query_hash = %h, error = ?e, "query failed");
            to_db_err("exec", e)
        })?;
        // affected_rows requires access via conn.affected_rows()
        Ok(conn.affected_rows())
    }

    #[instrument(level = "debug", skip(self, params, sql), fields(query_hash = %query_hash(sql)))]
    fn query(&self, sql: &str, params: &[SqlParam]) -> Result<SqlRows> {
        let mut conn = self.open()?;
        let params = params
            .iter()
            .map(sqlparam_to_mysql)
            .collect::<Result<Vec<Value>>>()?;
        let h = query_hash(sql);
        let mut result = conn.exec_iter(sql, params).map_err(|e| {
            error!(target = "airframe_mysql", query_hash = %h, error = ?e, "query failed");
            to_db_err("exec", e)
        })?;
        // We only take the first result set. Column names are optional for now.
        let columns: Vec<String> = Vec::new();
        let mut rows_out: Vec<Vec<SqlValue>> = Vec::new();
        while let Some(row) = result.next().transpose().map_err(|e| {
            error!(target = "airframe_mysql", query_hash = %h, error = ?e, "query failed");
            to_db_err("exec", e)
        })? {
            rows_out.push(map_row(&row));
        }
        Ok(SqlRows {
            columns,
            rows: rows_out,
        })
    }
}

fn map_row(row: &Row) -> Vec<SqlValue> {
    let mut out = Vec::with_capacity(row.len());
    for i in 0..row.len() {
        if let Some(v) = row.as_ref(i) {
            out.push(value_to_sqlvalue(v));
        }
    }
    out
}

fn value_to_sqlvalue(v: &Value) -> SqlValue {
    match v {
        Value::NULL => SqlValue::Null,
        Value::Bytes(b) => {
            // MySQL returns both text and blob as Bytes; we try UTF-8 decode, else keep bytes.
            match std::str::from_utf8(b) {
                Ok(s) => SqlValue::Str(s.to_string()),
                Err(_) => SqlValue::Bytes(b.clone()),
            }
        }
        Value::Int(i) => SqlValue::I64(*i),
        Value::UInt(u) => SqlValue::U64(*u),
        Value::Float(f) => SqlValue::F64(*f as f64),
        Value::Double(d) => SqlValue::F64(*d),
        Value::Date(y, m, d, h, min, s, micros) => {
            // Represent as string ISO-like for simplicity
            let s = format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:06}",
                y, m, d, h, min, s, micros
            );
            SqlValue::Str(s)
        }
        Value::Time(is_neg, days, hours, mins, secs, micros) => {
            let sign = if *is_neg { "-" } else { "" };
            let s = format!(
                "{}{} {:02}:{:02}:{:02}.{:06}",
                sign, days, hours, mins, secs, micros
            );
            SqlValue::Str(s)
        }
    }
}

fn sqlparam_to_mysql(p: &SqlParam) -> Result<Value> {
    Ok(match p {
        SqlParam::Null => Value::NULL,
        SqlParam::I64(v) => Value::Int(*v),
        SqlParam::U64(v) => Value::UInt(*v),
        SqlParam::F64(v) => Value::Double(*v),
        SqlParam::Bool(b) => Value::Int(if *b { 1 } else { 0 }),
        SqlParam::Str(s) => Value::Bytes(s.as_bytes().to_vec()),
        SqlParam::Bytes(b) => Value::Bytes((*b).to_vec()),
    })
}

#[derive(Clone)]
pub struct MySqlPool {
    url: String,
    default_db: Option<String>,
}

impl MySqlPool {
    pub fn new(url: impl Into<String>) -> Self {
        let this = Self {
            url: url.into(),
            default_db: None,
        };
        // No real pool; log init with conventional fields
        info!(
            target = "airframe_mysql",
            pool_max = 1,
            timeout_ms = 0,
            "mysql pool init"
        );
        this
    }
    pub fn with_db(url: impl Into<String>, db: impl Into<String>) -> Self {
        let this = Self {
            url: url.into(),
            default_db: Some(db.into()),
        };
        info!(
            target = "airframe_mysql",
            pool_max = 1,
            timeout_ms = 0,
            "mysql pool init"
        );
        this
    }
}

fn query_hash(sql: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    sql.hash(&mut hasher);
    hasher.finish()
}

impl DbPool for MySqlPool {
    type Conn = MySqlConn;
    fn get(&self) -> Result<Self::Conn> {
        Ok(MySqlConn {
            url: self.url.clone(),
            default_db: self.default_db.clone(),
        })
    }
}
