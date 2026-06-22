#![cfg(feature = "codec-shim")]

use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;

use crate::cache::byte::ByteCache;
use crate::cache::typed_cache::Cache;
use crate::error::{AirframeDataError, Result};
use crate::key::Key;

/// A typed Cache<V> implemented using an airframe_codec::Codec over a ByteCache.
/// This is a shim layer allowing reuse of codecs from the airframe_codec crate.
pub struct CodecCache<AC, BC>
where
    AC: airframe_codec::Codec + Send + Sync + 'static,
    BC: ByteCache,
{
    codec: Arc<AC>,
    inner: BC,
}

impl<AC, BC> Clone for CodecCache<AC, BC>
where
    AC: airframe_codec::Codec + Send + Sync + 'static,
    BC: ByteCache,
{
    fn clone(&self) -> Self {
        Self {
            codec: self.codec.clone(),
            inner: self.inner.clone(),
        }
    }
}

impl<AC, BC> CodecCache<AC, BC>
where
    AC: airframe_codec::Codec + Send + Sync + 'static,
    BC: ByteCache,
{
    pub fn new(codec: AC, inner: BC) -> Self {
        Self {
            codec: Arc::new(codec),
            inner,
        }
    }

    pub fn inner(&self) -> &BC {
        &self.inner
    }
    pub fn codec(&self) -> &AC {
        &self.codec
    }
}

impl<V, AC, BC> Cache<V> for CodecCache<AC, BC>
where
    V: Serialize + DeserializeOwned,
    AC: airframe_codec::Codec + Send + Sync + 'static,
    BC: ByteCache,
{
    fn put(&self, key: &Key, value: &V) -> Result<()> {
        let bytes = self
            .codec
            .encode(value)
            .map_err(|e| AirframeDataError::Codec(format!("airframe_codec encode: {}", e)))?;
        self.inner.put_bytes(key, &bytes)
    }

    fn get(&self, key: &Key) -> Result<Option<V>> {
        match self.inner.get_bytes(key)? {
            Some(b) => {
                let v = self.codec.decode(&b).map_err(|e| {
                    AirframeDataError::Codec(format!("airframe_codec decode: {}", e))
                })?;
                Ok(Some(v))
            }
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
    use crate::cache::byte::BackendByteCache;

    #[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
    struct Demo {
        a: u32,
        b: String,
    }

    #[test]
    fn roundtrip_bincode() {
        let bytes = BackendByteCache::new(MemBackend::new());
        // Use BincodeCodec from airframe_codec
        let ac = airframe_codec::codecs::BincodeCodec;
        let cache = CodecCache::new(ac, bytes);
        let key = Key::new("demo").unwrap();
        let val = Demo {
            a: 5,
            b: "five".into(),
        };
        <CodecCache<_, _> as Cache<Demo>>::put(&cache, &key, &val).unwrap();
        let out: Demo = <CodecCache<_, _> as Cache<Demo>>::get(&cache, &key)
            .unwrap()
            .unwrap();
        assert_eq!(out, val);
    }
}
