//! Core contracts and in-memory runtime for the modular Airframe framework.
//!
//! An Airframe application is assembled from [`Module`](module::Module)s that
//! declare the capabilities they *provide* and *require* through a
//! [`ModuleDescriptor`](module::ModuleDescriptor). The runtime resolves a valid
//! initialization order from those declarations, hands each module a
//! [`ModuleContext`](module::ModuleContext), and exposes a type-indexed
//! [`ServiceRegistry`](registry::ServiceRegistry) plus in-memory message buses
//! so modules can discover and talk to one another without hard-wiring.
//!
//! # Key pieces
//! - [`app::AppBuilder`] / [`app::AppHandle`] — assemble modules, resolve
//!   ordering, start the app, and reach services and buses at runtime.
//! - [`module::Module`] / [`module::ModuleDescriptor`] / [`module::ModuleContext`]
//!   — the contract every pluggable crate implements.
//! - [`registry::ServiceRegistry`] — register and fetch services by type.
//! - [`bus::EventBus`] / [`bus::CommandBus`] / [`bus::QueryBus`] — in-memory
//!   intra-app messaging.
//! - [`retry`] — backoff / retry helpers shared by adapter crates.
//!
//! # Example
//! ```ignore
//! use airframe_core::app::AppBuilder;
//!
//! # async fn run() -> anyhow::Result<()> {
//! // Compose the app from capability-providing modules; startup order is
//! // derived from each module's declared capabilities.
//! let app = AppBuilder::new()
//!     .with(/* a module */)
//!     .start()
//!     .await?;
//! // Resolved services are available by type:
//! // let svc = app.services.get::<MyService>();
//! # Ok(()) }
//! ```
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
