use std::sync::Arc;

use crate::cache::byte::ByteCache;
use crate::error::Result;
use crate::key::Key;

/// A simple read-through, write-through two-level cache.
#[derive(Clone)]
pub struct ReadThroughByteCache<Front: ByteCache, Back: ByteCache> {
    front: Front,
    back: Back,
    // optional counters
    hits: Arc<std::sync::atomic::AtomicU64>,
    misses: Arc<std::sync::atomic::AtomicU64>,
}

impl<Front: ByteCache, Back: ByteCache> ReadThroughByteCache<Front, Back> {
    pub fn new(front: Front, back: Back) -> Self {
        Self {
            front,
            back,
            hits: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            misses: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }
    pub fn counters(&self) -> (u64, u64) {
        (
            self.hits.load(std::sync::atomic::Ordering::Relaxed),
            self.misses.load(std::sync::atomic::Ordering::Relaxed),
        )
    }
}

impl<F: ByteCache, B: ByteCache> ByteCache for ReadThroughByteCache<F, B> {
    fn put_bytes(&self, key: &Key, bytes: &[u8]) -> Result<()> {
        // write-through: write both
        self.front.put_bytes(key, bytes)?;
        self.back.put_bytes(key, bytes)
    }
    fn get_bytes(&self, key: &Key) -> Result<Option<Vec<u8>>> {
        if let Some(b) = self.front.get_bytes(key)? {
            self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Ok(Some(b));
        }
        self.misses
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Some(b) = self.back.get_bytes(key)? {
            // populate front
            let _ = self.front.put_bytes(key, &b);
            Ok(Some(b))
        } else {
            Ok(None)
        }
    }
    fn remove(&self, key: &Key) -> Result<()> {
        // remove both
        self.front.remove(key)?;
        self.back.remove(key)
    }
    fn contains(&self, key: &Key) -> Result<bool> {
        if self.front.contains(key)? {
            return Ok(true);
        }
        self.back.contains(key)
    }
    fn list(&self) -> Result<Vec<Key>> {
        // authoritative set is the back
        self.back.list()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{LruByteCache, MemByteCache};

    #[test]
    fn populates_front_on_miss() {
        let front = LruByteCache::new(2);
        let back = MemByteCache::new();
        let c = ReadThroughByteCache::new(front.clone(), back.clone());
        let k = Key::new("foo").unwrap();
        back.put_bytes(&k, b"bar").unwrap();
        // miss in front, hit in back
        assert_eq!(c.get_bytes(&k).unwrap(), Some(b"bar".to_vec()));
        // now present in front
        assert_eq!(front.get_bytes(&k).unwrap(), Some(b"bar".to_vec()));
        let (h, m) = c.counters();
        assert_eq!(h, 0); // the first was a miss
        assert_eq!(m, 1);
    }
}
