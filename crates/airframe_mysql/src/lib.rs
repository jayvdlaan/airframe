/// MySQL adapter crate for Airframe.
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
