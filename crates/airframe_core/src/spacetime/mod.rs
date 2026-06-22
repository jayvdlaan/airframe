//! Unified Spacetime adapters and shims.
//!
//! This module provides a single generic adapter that can wrap either a
//! spacetime_core::Module (sync) or a spacetime_async_core::easy::AsyncModule
//! (async), depending on enabled features. The difference is erased via a
//! small shim trait with async methods.
// (Gated at the `mod spacetime;` site in lib.rs — no inner #![cfg] needed.)

mod adapter;
mod shim;
#[cfg(feature = "airframe-interop")]
mod shim_async;
mod shim_sync;

pub use adapter::StAsAf;
pub use shim::SpacetimeShim;
pub use shim_sync::SyncShim;

// Re-export std-backed Spacetime runtime types here to replace the old
// airframe_core::spacetime_adapter::{StdClock, StdTimer, StdRuntime} path.
// Downstream crates should import from airframe_core::spacetime::* going forward.
pub use spacetime_std_runtime::{StdClock, StdRuntime, StdTimer};
