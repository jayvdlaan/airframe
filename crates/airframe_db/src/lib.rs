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

/// Crate identity string.
pub const CRATE: &str = "airframe_db";

/// Simple readiness check placeholder.
pub fn ping() -> bool {
    true
}
