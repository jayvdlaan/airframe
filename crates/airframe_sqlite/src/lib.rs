//! Minimal SQLite adapter for the `airframe_db` traits.
//!
//! `airframe_sqlite` is a small, predictable SQLite adapter: a lightweight
//! `SqliteConn` that opens a real connection per operation and a cheap-to-clone
//! `SqlitePool` handle. Suitable for small utilities or as a building block.
//!
//! # Features
//! - `driver` — the `SqliteConn` / `SqlitePool` implementation.
//! - `module` — the `SqliteModule` that registers a pool and provides `cap:db`.
//!
//! # Example
//! ```ignore
//! // with feature "driver":
//! let pool = airframe_sqlite::SqlitePool::open("app.db")?;
//! ```
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
