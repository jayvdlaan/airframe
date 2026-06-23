//! PostgreSQL adapter for the `airframe_db` abstractions, backed by `sqlx`.
//!
//! `airframe_pg` provides an async PostgreSQL connection pool and integration
//! with the `airframe_db` SQL-execution surface, plus an optional Airframe
//! module providing `cap:db.pg`.
//!
//! # Features
//! - `driver` — the `PgPool` / `PgPoolOptions` connection pool.
//! - `module` — the `PgModule` that registers a pool and provides `cap:db.pg`.
//!
//! # Example
//! ```ignore
//! // with feature "driver":
//! let pool = airframe_pg::PgPool::connect("postgres://localhost/db").await?;
//! ```

pub mod error;

#[cfg(feature = "driver")]
pub mod pool;

#[cfg(feature = "driver")]
pub use pool::{PgPool, PgPoolOptions};

#[cfg(feature = "module")]
pub mod module;
#[cfg(feature = "module")]
pub use module::{PgModule, ServiceRegistryPgExt};
