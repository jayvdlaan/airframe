//! Application assembly: dependency-graph construction, the module resolver
//! (topological sort), and the app lifecycle (`AppBuilder`/`AppHandle`).
//!
//! The implementation is split across focused submodules:
//! - `graph` — module dependency graph plus the topo-sort resolver and
//!   optional layer validation.
//! - `lifecycle` — [`Bootstrap`], [`AppBuilder`], and [`AppHandle`]
//!   (start/shutdown and capability resolution).
//!
//! Everything is re-exported here so that `airframe_core::app::X` (and any
//! crate-root re-exports) keep working unchanged.

mod graph;
mod lifecycle;

pub use graph::{ModuleEdge, ModuleGraph, ModuleNode};
pub use lifecycle::{AppBuilder, AppHandle, Bootstrap};

#[cfg(test)]
mod tests;

// The test module (`tests.rs`) uses `use super::*;` and relies on these names
// being in scope from the `app` module, matching the pre-split layout. They are
// intentionally `use`d here (and only used by tests) to preserve that surface.
#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
use anyhow::Result;

#[cfg(test)]
use crate::bus::inmem::{InMemoryCommandBus, InMemoryEventBus};

#[cfg(test)]
use crate::module::{Module, ModuleContext};
