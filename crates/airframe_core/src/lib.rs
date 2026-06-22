pub mod app;
pub mod bus;
pub mod error;
pub mod module;
pub mod platform;
pub mod registry;
pub mod retry;
// Unified Spacetime module adapter (sync/async via shim)
#[cfg(feature = "airframe-spacetime")]
pub mod spacetime;
