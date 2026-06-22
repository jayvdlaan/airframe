use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::cache::byte::ByteCache;
use crate::error::Result;
use crate::key::Key;

/// A simple count-bounded in-memory LRU implementing ByteCache.
/// Not thread-contention optimized (O(n) moves), but fine for small capacities.
#[derive(Clone)]
pub struct LruByteCache {
    state: Arc<Mutex<State>>, // protects map+order
}

struct Entry {
    value: Vec<u8>,
}
struct State {
    cap: usize,
    map: HashMap<Key, Entry>,
    order: VecDeque<Key>, // front=oldest, back=newest
}

impl LruByteCache {
    pub fn new(capacity_entries: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                cap: capacity_entries.max(1),
                map: HashMap::new(),
                order: VecDeque::new(),
            })),
        }
    }

    fn touch_order(order: &mut VecDeque<Key>, key: &Key) {
        if let Some(pos) = order.iter().position(|k| k == key) {
            order.remove(pos);
        }
        order.push_back(key.clone());
    }

    fn evict_if_needed(state: &mut State) {
        while state.map.len() > state.cap {
            if let Some(old) = state.order.pop_front() {
                state.map.remove(&old);
            } else {
                break;
            }
        }
    }
}

impl ByteCache for LruByteCache {
    fn put_bytes(&self, key: &Key, bytes: &[u8]) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        s.map.insert(
            key.clone(),
            Entry {
                value: bytes.to_vec(),
            },
        );
        Self::touch_order(&mut s.order, key);
        Self::evict_if_needed(&mut s);
        Ok(())
    }
    fn get_bytes(&self, key: &Key) -> Result<Option<Vec<u8>>> {
        let mut s = self.state.lock().unwrap();
        let val = s.map.get(key).map(|e| e.value.clone());
        if val.is_some() {
            // move to MRU after cloning
            Self::touch_order(&mut s.order, key);
        }
        Ok(val)
    }
    fn remove(&self, key: &Key) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        s.map.remove(key);
        if let Some(pos) = s.order.iter().position(|k| k == key) {
            s.order.remove(pos);
        }
        Ok(())
    }
    fn contains(&self, key: &Key) -> Result<bool> {
        Ok(self.state.lock().unwrap().map.contains_key(key))
    }
    fn list(&self) -> Result<Vec<Key>> {
        Ok(self.state.lock().unwrap().map.keys().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lru_eviction_order() {
        let c = LruByteCache::new(2);
        let k1 = Key::new("k1").unwrap();
        let k2 = Key::new("k2").unwrap();
        let k3 = Key::new("k3").unwrap();
        c.put_bytes(&k1, b"1").unwrap();
        c.put_bytes(&k2, b"2").unwrap();
        // access k1 to make it MRU
        assert_eq!(c.get_bytes(&k1).unwrap(), Some(b"1".to_vec()));
        // insert k3 -> evict k2
        c.put_bytes(&k3, b"3").unwrap();
        assert!(c.contains(&k1).unwrap());
        assert!(!c.contains(&k2).unwrap());
        assert!(c.contains(&k3).unwrap());
    }
}
