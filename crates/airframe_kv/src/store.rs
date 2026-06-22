use std::{sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use tokio_stream::wrappers::ReceiverStream;

use airframe_core::bus::Event;
use airframe_core::registry::ServiceRegistry;

use crate::inmemory::InMemoryKvStore;

// Convenience accessors on ServiceRegistry for KV services.
pub trait ServiceRegistryKvExt {
    fn kv(&self) -> Option<Arc<dyn KvStore>>;
    fn kv_inmemory(&self) -> Option<Arc<InMemoryKvStore>>;
}
impl ServiceRegistryKvExt for ServiceRegistry {
    fn kv(&self) -> Option<Arc<dyn KvStore>> {
        self.get::<dyn KvStore>()
    }
    fn kv_inmemory(&self) -> Option<Arc<InMemoryKvStore>> {
        self.get::<InMemoryKvStore>()
    }
}

#[derive(Clone, Debug)]
pub struct KvMetadata {
    pub etag: u64,
    pub updated_by: String,
    pub updated_at: std::time::SystemTime,
    pub ttl: Option<Duration>,
}

#[derive(Clone, Debug)]
pub struct PutOptions {
    pub ttl: Option<Duration>,
    pub if_match: Option<u64>,
}

#[derive(Clone, Debug)]
pub enum PutResult {
    Created { etag: u64 },
    Updated { etag: u64 },
}

#[derive(Clone, Debug)]
pub enum DeleteResult {
    Deleted,
    NotFound,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum KvEvent {
    // ttl_ms is used instead of std::time::Duration to make the event serializable via serde_json
    Put {
        key: String,
        etag: u64,
        ttl_ms: Option<u64>,
    },
    Delete {
        key: String,
    },
    Expire {
        key: String,
    },
}
impl Event for KvEvent {
    const NAME: &'static str = "KvEvent";
}

// Pagination support
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[async_trait]
/// Trait for a key-value store with optional TTL, CAS (etag), prefix listing, pagination, and prefix watching.
/// Semantics:
/// - get/put/delete are atomic per key; put may enforce if_match etag for CAS.
/// - list_prefix yields a snapshot-like stream of current items under a prefix.
/// - list_prefix_paged returns a deterministic, lexicographically-ordered page; cursor is the last key of the previous page (exclusive).
/// - watch_prefix emits KvEvent::Put/Delete/Expire for keys under the prefix; ordering is best-effort.
pub trait KvStore: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<(Vec<u8>, KvMetadata)>>;
    async fn put(&self, key: &str, value: &[u8], opts: PutOptions) -> Result<PutResult>;
    async fn delete(&self, key: &str, if_match: Option<u64>) -> Result<DeleteResult>;
    fn list_prefix(&self, prefix: &str) -> Result<ReceiverStream<(String, Vec<u8>, KvMetadata)>>;
    fn list_prefix_paged(
        &self,
        prefix: &str,
        page_size: usize,
        cursor: Option<String>,
    ) -> Result<Page<(String, Vec<u8>, KvMetadata)>>;
    fn watch_prefix(&self, prefix: &str) -> Result<ReceiverStream<KvEvent>>;
}

#[async_trait]
pub trait KvStoreExt {
    async fn get_t<T: serde::de::DeserializeOwned + Send + 'static>(
        &self,
        key: &str,
    ) -> Result<Option<(T, KvMetadata)>>;
    async fn put_t<T: serde::Serialize + Send + Sync + 'static>(
        &self,
        key: &str,
        val: &T,
        opts: PutOptions,
    ) -> Result<PutResult>;
    /// Best-effort insert-if-absent helper. Returns true if a new value was inserted, false if the key already existed.
    async fn put_if_absent(&self, key: &str, value: &[u8], ttl: Option<Duration>) -> Result<bool>;
    /// Extend TTL for an existing key without changing value. Returns true if the key existed and TTL was updated.
    async fn touch(&self, key: &str, ttl: Duration) -> Result<bool>;
}

#[async_trait]
impl<T> KvStoreExt for T
where
    T: KvStore + ?Sized,
{
    async fn get_t<U: serde::de::DeserializeOwned + Send + 'static>(
        &self,
        key: &str,
    ) -> Result<Option<(U, KvMetadata)>> {
        if let Some((v, m)) = self.get(key).await? {
            let t = serde_json::from_slice::<U>(&v)?;
            Ok(Some((t, m)))
        } else {
            Ok(None)
        }
    }
    async fn put_t<U: serde::Serialize + Send + Sync + 'static>(
        &self,
        key: &str,
        val: &U,
        opts: PutOptions,
    ) -> Result<PutResult> {
        let bytes = serde_json::to_vec(val)?;
        self.put(key, &bytes, opts).await
    }
    async fn put_if_absent(&self, key: &str, value: &[u8], ttl: Option<Duration>) -> Result<bool> {
        if self.get(key).await?.is_some() {
            return Ok(false);
        }
        match self
            .put(
                key,
                value,
                PutOptions {
                    ttl,
                    if_match: None,
                },
            )
            .await?
        {
            PutResult::Created { .. } | PutResult::Updated { .. } => Ok(true),
        }
    }
    async fn touch(&self, key: &str, ttl: Duration) -> Result<bool> {
        if let Some((bytes, _meta)) = self.get(key).await? {
            let _ = self
                .put(
                    key,
                    &bytes,
                    PutOptions {
                        ttl: Some(ttl),
                        if_match: None,
                    },
                )
                .await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
