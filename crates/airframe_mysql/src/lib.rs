//! MySQL adapter for the `airframe_db` traits.
//!
//! `airframe_mysql` is an L4 IO adapter implementing the `airframe_db`
//! connection / pool / SQL-execution traits over MySQL, with an optional
//! Airframe module that provides `cap:db`.
//!
//! # Features
//! - `driver` — the `MySqlConn` / `MySqlPool` implementation.
//! - `module` — the `MySqlModule` that registers a pool and provides `cap:db`.
//!
//! # Example
//! ```ignore
//! // with feature "driver":
//! let pool = airframe_mysql::MySqlPool::connect("mysql://localhost/db").await?;
//! ```
/// Provides a synchronous connection and pool implementing airframe_db traits,
/// plus simple SQL execution helpers compatible with SqlExec.
pub const CRATE: &str = "airframe_mysql";

#[cfg(feature = "driver")]
pub mod conn;

#[cfg(feature = "driver")]
pub use conn::{MySqlConn, MySqlPool};

#[cfg(feature = "module")]
pub mod module;
#[cfg(feature = "module")]
pub use module::{MySqlModule, ServiceRegistryMySqlExt};

pub fn ping() -> bool {
    true
}
