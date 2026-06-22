use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::cache::byte::ByteCache;
use crate::error::Result;
use crate::key::Key;

#[derive(Clone, Default)]
pub struct MemByteCache {
    inner: Arc<RwLock<HashMap<Key, Vec<u8>>>>,
}

impl MemByteCache {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ByteCache for MemByteCache {
    fn put_bytes(&self, key: &Key, bytes: &[u8]) -> Result<()> {
        self.inner
            .write()
            .unwrap()
            .insert(key.clone(), bytes.to_vec());
        Ok(())
    }
    fn get_bytes(&self, key: &Key) -> Result<Option<Vec<u8>>> {
        Ok(self.inner.read().unwrap().get(key).cloned())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mem_cache_roundtrip() {
        let c = MemByteCache::new();
        let k = Key::new("a").unwrap();
        c.put_bytes(&k, b"hi").unwrap();
        assert!(c.contains(&k).unwrap());
        assert_eq!(c.get_bytes(&k).unwrap(), Some(b"hi".to_vec()));
        c.remove(&k).unwrap();
        assert!(!c.contains(&k).unwrap());
    }
}
