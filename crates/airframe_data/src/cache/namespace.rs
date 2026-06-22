use crate::cache::byte::ByteCache;
use crate::error::Result;
use crate::key::Key;

#[derive(Clone)]
pub struct NamespaceByteCache<BC: ByteCache> {
    inner: BC,
    prefix: String,
}

impl<BC: ByteCache> NamespaceByteCache<BC> {
    pub fn new(inner: BC, namespace: &str) -> Self {
        assert!(!namespace.is_empty(), "namespace cannot be empty");
        // validate namespace as a Key segment (no slashes or NUL)
        let _ = Key::new(namespace).expect("invalid namespace for Key");
        Self {
            inner,
            prefix: namespace.to_string(),
        }
    }

    fn wrap_key(&self, key: &Key) -> Key {
        // Use double-colon separator; colon is allowed by Key rules.
        Key::new(format!("{}::{}", self.prefix, key.as_str())).expect("wrapped key must be valid")
    }

    fn unwrap_key(&self, inner_key: &Key) -> Option<Key> {
        let s = inner_key.as_str();
        let p = format!("{}::", self.prefix);
        if let Some(rest) = s.strip_prefix(&p) {
            Key::new(rest).ok()
        } else {
            None
        }
    }
}

impl<BC: ByteCache> ByteCache for NamespaceByteCache<BC> {
    fn put_bytes(&self, key: &Key, bytes: &[u8]) -> Result<()> {
        let k = self.wrap_key(key);
        self.inner.put_bytes(&k, bytes)
    }
    fn get_bytes(&self, key: &Key) -> Result<Option<Vec<u8>>> {
        let k = self.wrap_key(key);
        self.inner.get_bytes(&k)
    }
    fn remove(&self, key: &Key) -> Result<()> {
        let k = self.wrap_key(key);
        self.inner.remove(&k)
    }
    fn contains(&self, key: &Key) -> Result<bool> {
        let k = self.wrap_key(key);
        self.inner.contains(&k)
    }
    fn list(&self) -> Result<Vec<Key>> {
        let mut out = Vec::new();
        for k in self.inner.list()? {
            if let Some(unwrapped) = self.unwrap_key(&k) {
                out.push(unwrapped);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::mem::MemByteCache;

    #[test]
    fn namespace_isolation() {
        let base = MemByteCache::new();
        let ns1 = NamespaceByteCache::new(base.clone(), "ns1");
        let ns2 = NamespaceByteCache::new(base, "ns2");
        let k = Key::new("k").unwrap();
        ns1.put_bytes(&k, b"a").unwrap();
        ns2.put_bytes(&k, b"b").unwrap();
        assert_eq!(ns1.get_bytes(&k).unwrap(), Some(b"a".to_vec()));
        assert_eq!(ns2.get_bytes(&k).unwrap(), Some(b"b".to_vec()));
        let l1 = ns1.list().unwrap();
        let l2 = ns2.list().unwrap();
        assert_eq!(l1, vec![k.clone()]);
        assert_eq!(l2, vec![k]);
    }
}
