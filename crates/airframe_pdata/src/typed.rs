use crate::bytes::PStoreBytes;
use crate::context::KeyResolver;
use crate::error::{AirframePdataError, Result};
use airframe_data::cache::ByteCache;
use airframe_data::codec::Codec;
use airframe_data::key::Key;
use serde::{de::DeserializeOwned, Serialize};

#[derive(Clone)]
pub struct PStore<C: Codec, BC: ByteCache, R: KeyResolver> {
    codec: C,
    inner: PStoreBytes<BC, R>,
}

impl<C: Codec, BC: ByteCache, R: KeyResolver> PStore<C, BC, R> {
    pub fn new(codec: C, inner: PStoreBytes<BC, R>) -> Self {
        Self { codec, inner }
    }

    pub fn put<T: Serialize>(&self, key: &Key, value: &T) -> Result<()> {
        let bytes = self
            .codec
            .encode(value)
            .map_err(|_| AirframePdataError::InvalidState)?;
        self.inner.put_bytes(key, &bytes)
    }

    pub fn get<T: DeserializeOwned>(&self, key: &Key) -> Result<Option<T>> {
        match self.inner.get_bytes(key)? {
            Some(b) => {
                let v = self
                    .codec
                    .decode(&b)
                    .map_err(|_| AirframePdataError::InvalidState)?;
                Ok(Some(v))
            }
            None => Ok(None),
        }
    }

    pub fn remove(&self, key: &Key) -> Result<()> {
        self.inner.remove(key)
    }
    pub fn contains(&self, key: &Key) -> Result<bool> {
        self.inner.contains(key)
    }
    pub fn list(&self) -> Result<Vec<Key>> {
        self.inner.list()
    }

    /// Re-encrypts the value for the given key using a new context.
    pub fn rewrap<R2: KeyResolver>(
        &self,
        key: &Key,
        new_ctx: &crate::context::PContext<R2>,
    ) -> Result<()> {
        match self.inner.rewrap_to(key, new_ctx)? {
            true => Ok(()),
            false => Err(AirframePdataError::InvalidState),
        }
    }

    pub fn inner(&self) -> &PStoreBytes<BC, R> {
        &self.inner
    }
}
