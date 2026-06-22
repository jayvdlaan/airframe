/// Minimal scaffold for airframe_sqlite.
/// This crate will contain a SQLite-backed KvBackend and SQL helpers.
pub const CRATE: &str = "airframe_sqlite";

pub mod error;

#[cfg(feature = "driver")]
pub mod conn;

#[cfg(feature = "driver")]
pub use conn::{SqliteConn, SqlitePool};

#[cfg(feature = "module")]
pub mod module;
#[cfg(feature = "module")]
pub use module::{ServiceRegistrySqliteExt, SqliteModule};

pub fn ping() -> bool {
    true
}
