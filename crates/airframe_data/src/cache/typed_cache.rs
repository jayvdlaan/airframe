use super::byte::ByteCache;
use crate::codec::Codec;
use crate::error::Result;
use crate::key::Key;
use serde::{de::DeserializeOwned, Serialize};

/// Typed cache over some value V.
pub trait Cache<V>: Clone + Send + Sync + 'static {
    fn put(&self, key: &Key, value: &V) -> Result<()>;
    fn get(&self, key: &Key) -> Result<Option<V>>;
    fn remove(&self, key: &Key) -> Result<()>;
    fn contains(&self, key: &Key) -> Result<bool>;
    fn list(&self) -> Result<Vec<Key>>;
}

/// Bridge from Codec + ByteCache to a typed Cache.
#[derive(Clone)]
pub struct SerdeCache<C: Codec, BC: ByteCache> {
    codec: C,
    inner: BC,
}

impl<C: Codec, BC: ByteCache> SerdeCache<C, BC> {
    pub fn new(codec: C, inner: BC) -> Self {
        Self { codec, inner }
    }
    pub fn codec(&self) -> &C {
        &self.codec
    }
    pub fn inner(&self) -> &BC {
        &self.inner
    }
}

impl<V, C, BC> Cache<V> for SerdeCache<C, BC>
where
    V: Serialize + DeserializeOwned,
    C: Codec,
    BC: ByteCache,
{
    fn put(&self, key: &Key, value: &V) -> Result<()> {
        let bytes = self.codec.encode(value)?;
        self.inner.put_bytes(key, &bytes)
    }
    fn get(&self, key: &Key) -> Result<Option<V>> {
        match self.inner.get_bytes(key)? {
            Some(b) => Ok(Some(self.codec.decode(&b)?)),
            None => Ok(None),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::mem::MemBackend;
    use crate::codec::JsonCodec;
    use crate::key::Key;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct Demo {
        a: u32,
        b: String,
    }

    #[test]
    fn codec_cache_roundtrip() {
        let backend = MemBackend::new();
        let bytes = crate::cache::byte::BackendByteCache::new(backend);
        let codec = JsonCodec;
        let cache = SerdeCache::new(codec, bytes);

        let key = Key::new("demo").unwrap();
        let val = Demo {
            a: 7,
            b: "seven".into(),
        };
        cache.put(&key, &val).unwrap();
        // Disambiguate the generic Cache<V> using UFCS on the concrete type
        type CC = SerdeCache<JsonCodec, crate::cache::byte::BackendByteCache<MemBackend>>;
        assert!(<CC as Cache<Demo>>::contains(&cache, &key).unwrap());
        let out: Demo = <CC as Cache<Demo>>::get(&cache, &key).unwrap().unwrap();
        assert_eq!(out, val);
        let keys = <CC as Cache<Demo>>::list(&cache).unwrap();
        assert_eq!(keys, vec![key.clone()]);
        <CC as Cache<Demo>>::remove(&cache, &key).unwrap();
        assert!(!<CC as Cache<Demo>>::contains(&cache, &key).unwrap());
    }
}
