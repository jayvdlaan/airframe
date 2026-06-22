use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::cache::byte::ByteCache;
use crate::error::Result;
use crate::key::Key;

#[derive(Clone)]
pub struct TtlByteCache<BC: ByteCache> {
    inner: BC,
    default_ttl: Option<Duration>,
    expiries: Arc<RwLock<HashMap<Key, Instant>>>,
}

impl<BC: ByteCache> TtlByteCache<BC> {
    pub fn new(inner: BC, default_ttl: Option<Duration>) -> Self {
        Self {
            inner,
            default_ttl,
            expiries: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    pub fn with_ttl(inner: BC, ttl: Duration) -> Self {
        Self::new(inner, Some(ttl))
    }

    fn is_expired(&self, key: &Key) -> bool {
        if let Some(exp) = self.expiries.read().unwrap().get(key).cloned() {
            Instant::now() >= exp
        } else {
            false
        }
    }

    fn purge_if_expired(&self, key: &Key) -> Result<()> {
        if self.is_expired(key) {
            self.inner.remove(key)?;
            self.expiries.write().unwrap().remove(key);
        }
        Ok(())
    }

    pub fn put_with_ttl(&self, key: &Key, ttl: Duration, bytes: &[u8]) -> Result<()> {
        self.inner.put_bytes(key, bytes)?;
        self.expiries
            .write()
            .unwrap()
            .insert(key.clone(), Instant::now() + ttl);
        Ok(())
    }
}

impl<BC: ByteCache> ByteCache for TtlByteCache<BC> {
    fn put_bytes(&self, key: &Key, bytes: &[u8]) -> Result<()> {
        self.inner.put_bytes(key, bytes)?;
        if let Some(ttl) = self.default_ttl {
            self.expiries
                .write()
                .unwrap()
                .insert(key.clone(), Instant::now() + ttl);
        }
        Ok(())
    }
    fn get_bytes(&self, key: &Key) -> Result<Option<Vec<u8>>> {
        self.purge_if_expired(key)?;
        self.inner.get_bytes(key)
    }
    fn remove(&self, key: &Key) -> Result<()> {
        self.expiries.write().unwrap().remove(key);
        self.inner.remove(key)
    }
    fn contains(&self, key: &Key) -> Result<bool> {
        self.purge_if_expired(key)?;
        self.inner.contains(key)
    }
    fn list(&self) -> Result<Vec<Key>> {
        // best-effort filter expired
        let keys = self.inner.list()?;
        for k in &keys {
            let _ = self.purge_if_expired(k);
        }
        Ok(keys.into_iter().filter(|k| !self.is_expired(k)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::mem::MemByteCache;
    use std::thread::sleep;

    #[test]
    fn ttl_expires() {
        let base = MemByteCache::new();
        let c = TtlByteCache::with_ttl(base, Duration::from_millis(50));
        let k = Key::new("x").unwrap();
        c.put_bytes(&k, b"val").unwrap();
        assert!(c.contains(&k).unwrap());
        sleep(Duration::from_millis(60));
        assert!(c.get_bytes(&k).unwrap().is_none());
        assert!(!c.contains(&k).unwrap());
    }
}
