use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tokio_stream::wrappers::ReceiverStream;

use crate::store::{DeleteResult, KvEvent, KvMetadata, KvStore, Page, PutOptions, PutResult};

#[derive(Clone, Debug)]
pub enum AclMode {
    Warn,
    Enforce,
}

#[derive(Clone)]
pub struct KvStoreAcl {
    inner: Arc<dyn KvStore>,
    allow: Arc<Vec<String>>, // prefixes
    mode: AclMode,
}

impl KvStoreAcl {
    pub fn new(inner: Arc<dyn KvStore>, allow: Vec<String>, mode: AclMode) -> Self {
        Self {
            inner,
            allow: Arc::new(allow),
            mode,
        }
    }
    fn allowed(&self, key: &str) -> bool {
        self.allow.iter().any(|p| key.starts_with(p))
    }
}

#[async_trait]
impl KvStore for KvStoreAcl {
    async fn get(&self, key: &str) -> Result<Option<(Vec<u8>, KvMetadata)>> {
        self.inner.get(key).await
    }
    async fn put(&self, key: &str, value: &[u8], opts: PutOptions) -> Result<PutResult> {
        if !self.allowed(key) {
            match self.mode {
                AclMode::Warn => {
                    tracing::warn!(target = "airframe_kv", key, "KV write outside allowlist");
                }
                AclMode::Enforce => {
                    return Err(anyhow!("write to disallowed key prefix: {}", key));
                }
            }
        }
        self.inner.put(key, value, opts).await
    }
    async fn delete(&self, key: &str, if_match: Option<u64>) -> Result<DeleteResult> {
        if !self.allowed(key) {
            match self.mode {
                AclMode::Warn => {
                    tracing::warn!(target = "airframe_kv", key, "KV delete outside allowlist");
                }
                AclMode::Enforce => {
                    return Err(anyhow!("delete of disallowed key prefix: {}", key));
                }
            }
        }
        self.inner.delete(key, if_match).await
    }
    fn list_prefix(&self, prefix: &str) -> Result<ReceiverStream<(String, Vec<u8>, KvMetadata)>> {
        self.inner.list_prefix(prefix)
    }
    fn list_prefix_paged(
        &self,
        prefix: &str,
        page_size: usize,
        cursor: Option<String>,
    ) -> Result<Page<(String, Vec<u8>, KvMetadata)>> {
        self.inner.list_prefix_paged(prefix, page_size, cursor)
    }
    fn watch_prefix(&self, prefix: &str) -> Result<ReceiverStream<KvEvent>> {
        self.inner.watch_prefix(prefix)
    }
}
