/// Minimal scaffold for airframe_pg.
/// This crate provides a PostgreSQL-backed adapter for airframe_db abstractions.
pub const CRATE: &str = "airframe_pg";

pub mod error;

#[cfg(feature = "driver")]
pub mod pool;

#[cfg(feature = "driver")]
pub use pool::{PgPool, PgPoolOptions};

#[cfg(feature = "module")]
pub mod module;
#[cfg(feature = "module")]
pub use module::{PgModule, ServiceRegistryPgExt};

pub fn ping() -> bool {
    true
}
