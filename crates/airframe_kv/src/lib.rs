//! Shared, observable key-value store for inter-module coordination in Airframe.
//!
//! `airframe_kv` provides an in-memory key-value store with TTL expiry,
//! compare-and-set via etags, prefix listing, and prefix watches over async
//! streams. As a module it registers the store and forwards change events to the
//! app `EventBus` so other modules can react.
//!
//! # Key pieces
//! - [`InMemoryKvStore`] — the default in-memory store.
//! - [`KvModule`] — Airframe module providing `cap:kv`.
//! - [`KvStoreAcl`] / [`AclMode`] — an access-control wrapper over a store.
//! - [`PrefixEvent`] — change events delivered to prefix watchers.
//! - `FilesystemKvStore` — a persistent backend, under the `kv-fs` feature.
//!
//! # Example
//! ```ignore
//! use airframe_kv::InMemoryKvStore;
//!
//! let kv = InMemoryKvStore::new();
//! kv.put("k", b"v".to_vec(), None).await?;
//! let got = kv.get("k").await?;
//! ```
mod acl;
mod inmemory;
mod module;
mod store;
mod watch;

pub use acl::{AclMode, KvStoreAcl};
pub use inmemory::InMemoryKvStore;
pub use module::KvModule;
pub use store::{
    DeleteResult, KvEvent, KvMetadata, KvStore, KvStoreExt, Page, PutOptions, PutResult,
    ServiceRegistryKvExt,
};
pub use watch::{kv_watch_prefix_t, kv_watch_prefix_t_with_deletes, PrefixEvent};

// --- Filesystem backend (feature-gated) ---
#[cfg(feature = "kv-fs")]
pub mod filesystem;
#[cfg(feature = "kv-fs")]
pub use filesystem::FilesystemKvStore;

#[cfg(test)]
mod tests;
