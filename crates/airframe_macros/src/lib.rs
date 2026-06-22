//! Declarative macros for Airframe module development.
//!
//! This crate provides macros to reduce boilerplate when building Airframe modules:
//!
//! - [`module_descriptor!`] - Create `ModuleDescriptor` instances with named fields
//! - [`register_service!`] - Register services in a `ServiceRegistry`
//! - [`service_ext!`] - Define extension traits for typed service access
//! - [`caps!`] - Convert typed `Cap` values to capability string slices
//!
//! # Examples
//!
//! ## Module Descriptor
//!
//! ```ignore
//! use airframe_macros::module_descriptor;
//!
//! let desc = module_descriptor!(
//!     name: "my_module",
//!     version: "1.0.0",
//!     provides: ["cap:my.service"],
//!     requires: ["cap:config"],
//! );
//! ```
//!
//! ## Service Registration
//!
//! ```ignore
//! use airframe_macros::register_service;
//!
//! register_service!(registry, MyService::new());
//! ```
//!
//! ## Capabilities
//!
//! ```ignore
//! use airframe_macros::caps;
//! use airframe_core::module::{CAP_HTTP_SERVER, CAP_CONFIG};
//!
//! let provides = caps![CAP_HTTP_SERVER, CAP_CONFIG];
//! ```

// Declarative macros are exported via #[macro_export] and available at crate root
mod caps;
mod descriptor;
mod error;
mod registry;

// Re-export types needed by macros for hygiene
#[doc(hidden)]
pub use airframe_core::error::AirframeError as __AirframeError;
#[doc(hidden)]
pub use airframe_core::error::ErrorRange as __ErrorRange;
#[doc(hidden)]
pub use airframe_core::module::ModuleDescriptor;
