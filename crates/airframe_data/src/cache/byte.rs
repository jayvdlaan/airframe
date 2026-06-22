use crate::backend::KvBackend;
use crate::error::Result;
use crate::key::Key;
use tracing::debug;

/// Bytes-level cache abstraction. Mirrors KvBackend but allows layering
/// of cache-specific behaviors (LRU, TTL, compression, encryption, etc.).
pub trait ByteCache: Clone + Send + Sync + 'static {
    fn put_bytes(&self, key: &Key, bytes: &[u8]) -> Result<()>;
    fn get_bytes(&self, key: &Key) -> Result<Option<Vec<u8>>>;
    fn remove(&self, key: &Key) -> Result<()>;
    fn contains(&self, key: &Key) -> Result<bool>;
    fn list(&self) -> Result<Vec<Key>>;
}

/// Adapter that exposes any KvBackend as a ByteCache.
#[derive(Clone)]
pub struct BackendByteCache<B: KvBackend + Clone> {
    inner: B,
}

impl<B: KvBackend + Clone> BackendByteCache<B> {
    pub fn new(inner: B) -> Self {
        Self { inner }
    }
    pub fn into_inner(self) -> B {
        self.inner
    }
    pub fn inner(&self) -> &B {
        &self.inner
    }
}

impl<B: KvBackend + Clone> ByteCache for BackendByteCache<B> {
    fn put_bytes(&self, key: &Key, bytes: &[u8]) -> Result<()> {
        self.inner.put_bytes(key, bytes)
    }
    fn get_bytes(&self, key: &Key) -> Result<Option<Vec<u8>>> {
        match self.inner.get_bytes(key)? {
            Some(v) => {
                debug!(target = "airframe_data", key = %key, "cache hit");
                Ok(Some(v))
            }
            None => {
                debug!(target = "airframe_data", key = %key, "cache miss");
                Ok(None)
            }
        }
    }
    fn remove(&self, key: &Key) -> Result<()> {
        self.inner.remove(key)
    }
    fn contains(&self, key: &Key) -> Result<bool> {
        self.inner.contains(key)
    }
    fn list(&self) -> Result<Vec<Key>> {
        self.inner.list()
    }
}
