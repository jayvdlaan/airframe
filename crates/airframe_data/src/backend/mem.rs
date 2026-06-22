#[derive(Clone, Default)]
pub struct MemBackend {
    inner: Arc<RwLock<HashMap<Key, Vec<u8>>>>,
}

impl MemBackend {
    pub fn new() -> Self {
        Self::default()
    }
}

impl KvBackend for MemBackend {
    #[instrument(level = "debug", skip(self, buf))]
    fn put_bytes(&self, key: &Key, buf: &[u8]) -> Result<()> {
        self.inner
            .write()
            .unwrap()
            .insert(key.clone(), buf.to_vec());
        Ok(())
    }

    #[instrument(level = "debug", skip(self))]
    fn get_bytes(&self, key: &Key) -> Result<Option<Vec<u8>>> {
        Ok(self.inner.read().unwrap().get(key).cloned())
    }

    #[instrument(level = "debug", skip(self))]
    fn remove(&self, key: &Key) -> Result<()> {
        self.inner.write().unwrap().remove(key);
        Ok(())
    }

    fn contains(&self, key: &Key) -> Result<bool> {
        Ok(self.inner.read().unwrap().contains_key(key))
    }

    fn list(&self) -> Result<Vec<Key>> {
        Ok(self.inner.read().unwrap().keys().cloned().collect())
    }
}

use crate::backend::KvBackend;
use crate::error::Result;
use crate::key::Key;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::instrument;
