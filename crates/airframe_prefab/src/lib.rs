//! Airframe prefabs: minimal, opinionated AppBuilder presets.
//! This crate provides small constructors that return an AppBuilder
//! pre-wired with sensible defaults for common application types.
//!
//! Initial scope: CLI and Service prefabs.

#![forbid(unsafe_code)]

// Public modules split out of this crate
pub mod cli;
pub mod worker;

// HTTP-only modules
#[cfg(feature = "http")]
pub mod gateway;
#[cfg(feature = "http")]
pub mod http_cors;
#[cfg(all(feature = "http", feature = "openapi"))]
pub mod http_openapi;
#[cfg(feature = "http")]
pub mod http_spa;
#[cfg(feature = "http")]
pub mod http_static_files;

// Prefab constructors (builders) split into a submodule but re-exported here
mod prefabs;
pub use prefabs::*;

// Re-export config defaults contributor API for ergonomics
#[cfg(feature = "config")]
pub use airframe_config::{
    get_or_create_config_defaults_registry, ConfigDefaultsContributor, ConfigDefaultsRegistry,
};
