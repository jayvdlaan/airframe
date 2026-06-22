#![cfg(feature = "driver")]

use std::time::Duration;

use airframe_db::connection::{SqlExec, SqlParam, SqlRows, SqlValue};
use airframe_db::error::{AirframeDbError, Result};
use sqlx::postgres::{PgPoolOptions as SqlxPgPoolOptions, PgRow};
use sqlx::{Column, Row, TypeInfo};
use tracing::{error, info, instrument};

/// Configuration options for the PostgreSQL connection pool.
#[derive(Debug, Clone)]
pub struct PgPoolOptions {
    /// Minimum number of idle connections to maintain (default: 1).
    pub min_connections: u32,
    /// Maximum number of connections in the pool (default: 10).
    pub max_connections: u32,
    /// Timeout in seconds for acquiring a connection (default: 5).
    pub connect_timeout_secs: u64,
}

impl Default for PgPoolOptions {
    fn default() -> Self {
        Self {
            min_connections: 1,
            max_connections: 10,
            connect_timeout_secs: 5,
        }
    }
}

/// A PostgreSQL connection pool backed by sqlx.
#[derive(Clone)]
pub struct PgPool {
    pool: sqlx::PgPool,
}

impl PgPool {
    /// Connect to a PostgreSQL database with the given URL and pool options.
    pub async fn connect(url: &str, opts: PgPoolOptions) -> Result<Self> {
        info!(
            target = "airframe_pg",
            min = opts.min_connections,
            max = opts.max_connections,
            timeout_s = opts.connect_timeout_secs,
            "connecting to postgres"
        );

        let pool = SqlxPgPoolOptions::new()
            .min_connections(opts.min_connections)
            .max_connections(opts.max_connections)
            .acquire_timeout(Duration::from_secs(opts.connect_timeout_secs))
            .connect(url)
            .await
            .map_err(|e| AirframeDbError::Connection(e.to_string()))?;

        info!(target = "airframe_pg", "postgres pool connected");
        Ok(Self { pool })
    }

    /// Lightweight liveness check — acquires and releases a connection.
    pub async fn ping(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|e| AirframeDbError::Connection(e.to_string()))?;
        Ok(())
    }

    /// Execute a statement, returning the number of affected rows.
    ///
    /// Uses PostgreSQL positional parameters (`$1`, `$2`, ...).
    #[instrument(level = "debug", skip(self, params, sql))]
    pub async fn execute(&self, sql: &str, params: &[SqlParam<'_>]) -> Result<u64> {
        let mut q = sqlx::query(sql);
        for param in params {
            q = bind_param(q, param);
        }
        let result = q.execute(&self.pool).await.map_err(|e| {
            error!(target = "airframe_pg", error = ?e, "execute failed");
            AirframeDbError::InvalidState
        })?;
        Ok(result.rows_affected())
    }

    /// Run a query returning rows in an adapter-agnostic representation.
    ///
    /// Uses PostgreSQL positional parameters (`$1`, `$2`, ...).
    #[instrument(level = "debug", skip(self, params, sql))]
    pub async fn query(&self, sql: &str, params: &[SqlParam<'_>]) -> Result<SqlRows> {
        let mut q = sqlx::query(sql);
        for param in params {
            q = bind_param(q, param);
        }
        let rows: Vec<PgRow> = q.fetch_all(&self.pool).await.map_err(|e| {
            error!(target = "airframe_pg", error = ?e, "query failed");
            AirframeDbError::InvalidState
        })?;

        if rows.is_empty() {
            return Ok(SqlRows::default());
        }

        let columns: Vec<String> = rows[0]
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        let mut out_rows = Vec::with_capacity(rows.len());
        for row in &rows {
            let mut out_row = Vec::with_capacity(columns.len());
            for (i, col) in row.columns().iter().enumerate() {
                let value = decode_column(row, i, col.type_info());
                out_row.push(value);
            }
            out_rows.push(out_row);
        }

        Ok(SqlRows {
            columns,
            rows: out_rows,
        })
    }

    /// Run sqlx migrations from the given directory path.
    pub async fn run_migrations(&self, path: &str) -> Result<()> {
        info!(target = "airframe_pg", path = %path, "running migrations");
        let migrator = sqlx::migrate::Migrator::new(std::path::Path::new(path))
            .await
            .map_err(|e| AirframeDbError::Migration(e.to_string()))?;
        migrator
            .run(&self.pool)
            .await
            .map_err(|e| AirframeDbError::Migration(e.to_string()))?;
        info!(target = "airframe_pg", "migrations complete");
        Ok(())
    }

    /// Synchronous execute that blocks on the async runtime.
    /// Intended for implementing the sync `SqlExec` trait.
    fn execute_sync(&self, sql: &str, params: &[SqlParam<'_>]) -> Result<u64> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.execute(sql, params))
        })
    }

    /// Synchronous query that blocks on the async runtime.
    /// Intended for implementing the sync `SqlExec` trait.
    fn query_sync(&self, sql: &str, params: &[SqlParam<'_>]) -> Result<SqlRows> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.query(sql, params))
        })
    }
}

/// Implement the synchronous `SqlExec` trait by blocking on the async pool.
///
/// This requires a tokio runtime context. Callers using async code should
/// prefer the async methods on `PgPool` directly.
impl SqlExec for PgPool {
    fn execute(&self, sql: &str, params: &[SqlParam<'_>]) -> Result<u64> {
        self.execute_sync(sql, params)
    }

    fn query(&self, sql: &str, params: &[SqlParam<'_>]) -> Result<SqlRows> {
        self.query_sync(sql, params)
    }
}

/// Bind a single `SqlParam` to a sqlx query.
///
/// sqlx's `Query::bind` is generic over the bind type. Each `bind()` call
/// consumes the query and returns a new one with a different type parameter,
/// but since we use `sqlx::query()` (runtime-checked), the erased `Query`
/// type keeps a uniform signature through `bind(value)` for common types.
fn bind_param<'q>(
    q: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    param: &'q SqlParam<'q>,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    match param {
        SqlParam::Null => q.bind(None::<i64>),
        SqlParam::I64(v) => q.bind(*v),
        SqlParam::U64(v) => q.bind(*v as i64), // Postgres has no unsigned int type
        SqlParam::F64(v) => q.bind(*v),
        SqlParam::Bool(v) => q.bind(*v),
        SqlParam::Str(v) => q.bind(*v),
        SqlParam::Bytes(v) => q.bind(*v),
    }
}

/// Decode a single column value from a PgRow into a SqlValue.
fn decode_column(row: &PgRow, idx: usize, type_info: &sqlx::postgres::PgTypeInfo) -> SqlValue {
    let type_name = type_info.name();

    match type_name {
        "BOOL" => match row.try_get::<Option<bool>, _>(idx) {
            Ok(Some(v)) => SqlValue::Bool(v),
            _ => SqlValue::Null,
        },
        "INT2" | "SMALLINT" => match row.try_get::<Option<i16>, _>(idx) {
            Ok(Some(v)) => SqlValue::I64(v as i64),
            _ => SqlValue::Null,
        },
        "INT4" | "INT" | "INTEGER" => match row.try_get::<Option<i32>, _>(idx) {
            Ok(Some(v)) => SqlValue::I64(v as i64),
            _ => SqlValue::Null,
        },
        "INT8" | "BIGINT" => match row.try_get::<Option<i64>, _>(idx) {
            Ok(Some(v)) => SqlValue::I64(v),
            _ => SqlValue::Null,
        },
        "FLOAT4" | "REAL" => match row.try_get::<Option<f32>, _>(idx) {
            Ok(Some(v)) => SqlValue::F64(v as f64),
            _ => SqlValue::Null,
        },
        "FLOAT8" | "DOUBLE PRECISION" => match row.try_get::<Option<f64>, _>(idx) {
            Ok(Some(v)) => SqlValue::F64(v),
            _ => SqlValue::Null,
        },
        "BYTEA" => match row.try_get::<Option<Vec<u8>>, _>(idx) {
            Ok(Some(v)) => SqlValue::Bytes(v),
            _ => SqlValue::Null,
        },
        // TEXT, VARCHAR, CHAR, NAME, and all other text-like types
        _ => match row.try_get::<Option<String>, _>(idx) {
            Ok(Some(v)) => SqlValue::Str(v),
            _ => SqlValue::Null,
        },
    }
}
