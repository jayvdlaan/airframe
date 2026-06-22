//! airframe_config prelude: convenient re-exports for common types.
//! Import with: `use airframe_config::prelude::*;`

pub use crate::api::types::{BasicConfig, ConfigReloaded};

#[cfg(feature = "module")]
pub use crate::module::ConfigModule;
