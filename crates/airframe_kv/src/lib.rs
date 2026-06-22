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
