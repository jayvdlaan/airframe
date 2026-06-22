#![cfg(feature = "integration-compress")]

use crate::cache::byte::ByteCache;
use crate::error::{AirframeDataError, Result};
use crate::key::Key;
use airframe_compress::{AirframeCompressError, Compressor};

#[derive(Clone)]
pub struct CompressByteCache<BC: ByteCache, CC: Compressor> {
    inner: BC,
    algo: CC,
}

impl<BC: ByteCache, CC: Compressor> CompressByteCache<BC, CC> {
    pub fn new(inner: BC, algo: CC) -> Self {
        Self { inner, algo }
    }
}

impl<BC: ByteCache, CC: Compressor + Clone> ByteCache for CompressByteCache<BC, CC> {
    fn put_bytes(&self, key: &Key, bytes: &[u8]) -> Result<()> {
        let compressed = self.algo.compress(bytes).map_err(|e| {
            AirframeDataError::Codec(format!("compress {}: {}", self.algo.name(), e))
        })?;
        self.inner.put_bytes(key, &compressed)
    }
    fn get_bytes(&self, key: &Key) -> Result<Option<Vec<u8>>> {
        match self.inner.get_bytes(key)? {
            Some(c) => {
                let out = self.algo.decompress(&c).map_err(|e| {
                    AirframeDataError::Codec(format!("decompress {}: {}", self.algo.name(), e))
                })?;
                Ok(Some(out))
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
    use crate::cache::mem::MemByteCache;

    #[test]
    fn roundtrip_compress_zstd() {
        // This test only compiles/runs when feature "compress" is enabled and zstd is available by default in airframe_compress
        let base = MemByteCache::new();
        let algo = airframe_compress::Zstd::new(3);
        let c = CompressByteCache::new(base, algo);
        let k = Key::new("k").unwrap();
        let data = b"hello hello hello".to_vec();
        c.put_bytes(&k, &data).unwrap();
        let out = c.get_bytes(&k).unwrap().unwrap();
        assert_eq!(out, data);
    }
}
