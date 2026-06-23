//! Minimal, composable database traits for Airframe, implemented by adapter crates.
//!
//! `airframe_db` defines the contracts for database access — connections, pools,
//! SQL execution, transactions, and migrations — plus retry/timeout helpers,
//! leaving the actual driver to adapter crates such as `airframe_sqlite`,
//! `airframe_mysql`, and `airframe_pg`.
//!
//! # Key pieces
//! - [`DbConnection`] / [`DbPool`] / [`DbTx`] — connection, pool, and transaction traits.
//! - [`SqlExec`] / [`SqlParam`] / [`SqlRows`] — the SQL execution surface.
//! - [`Migrator`] — schema migration trait.
//! - [`Driver`] / [`connect`] — driver registration and the connection entry point.
//! - [`ConnectionString`] / [`PoolConfig`] — parsed connection config (with redaction).
//! - [`AirframeDbError`] — the crate error type.
//!
//! # Example
//! ```ignore
//! use airframe_db::{parse_connection_string, ConnectionString};
//!
//! let cs: ConnectionString = parse_connection_string("sqlite://app.db")?;
//! ```
pub mod config;
pub mod connection;
pub mod driver;
pub mod error;
pub mod pool;
pub mod retry;
pub mod timeout;

#[cfg(feature = "module")]
pub mod module;

pub use config::{parse_connection_string, ConnectionString, PoolConfig};
pub use connection::{DbConnection, DbPool, DbTx, Migrator, SqlExec, SqlParam, SqlRows, SqlValue};
pub use driver::{connect, Driver};
pub use error::{AirframeDbError, Result};
pub use pool::{wait_until_ready, NewConnPool};
pub use retry::{retry, RetryPolicy};
pub use timeout::run_with_timeout;

#[cfg(feature = "module")]
pub use module::{DbConfig, DbDriverId, DbHandle, DbModule, MigrationsMode};
