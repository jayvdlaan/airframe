use serde::{de::DeserializeOwned, Serialize};

use crate::backend::KvBackend;
use crate::codec::Codec;
use crate::error::Result;
use crate::key::Key;
use std::any::type_name;
use tracing::info;

#[derive(Clone)]
pub struct Repo<B: KvBackend, C: Codec> {
    backend: B,
    codec: C,
}

impl<B: KvBackend, C: Codec> Repo<B, C> {
    pub fn new(backend: B, codec: C) -> Self {
        Self { backend, codec }
    }

    pub fn put<T: Serialize>(&self, key: &Key, value: &T) -> Result<()> {
        let bytes = self.codec.encode(value)?;
        self.backend.put_bytes(key, &bytes)
    }

    pub fn get<T: DeserializeOwned>(&self, key: &Key) -> Result<Option<T>> {
        match self.backend.get_bytes(key)? {
            Some(bytes) => {
                let value = self.codec.decode(&bytes)?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    pub fn remove(&self, key: &Key) -> Result<()> {
        self.backend.remove(key)
    }
    pub fn contains(&self, key: &Key) -> Result<bool> {
        self.backend.contains(key)
    }
    pub fn list(&self) -> Result<Vec<Key>> {
        self.backend.list()
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }
    pub fn codec(&self) -> &C {
        &self.codec
    }
}

// Builder that anticipates future crypto/schema layering without exposing it yet.
#[derive(Default)]
pub struct RepoBuilder<B, C> {
    backend: Option<B>,
    codec: Option<C>,
}

impl<B: KvBackend, C: Codec> RepoBuilder<B, C> {
    pub fn new() -> Self {
        Self {
            backend: None,
            codec: None,
        }
    }
    pub fn backend(mut self, backend: B) -> Self {
        // Log backend selection
        let name = type_name::<B>();
        info!(target = "airframe_data", backend = %name, "backend selected");
        self.backend = Some(backend);
        self
    }
    pub fn codec(mut self, codec: C) -> Self {
        self.codec = Some(codec);
        self
    }
    pub fn build(self) -> Result<Repo<B, C>> {
        let backend = self
            .backend
            .ok_or(crate::error::AirframeDataError::InvalidState)?;
        let codec = self
            .codec
            .ok_or(crate::error::AirframeDataError::InvalidState)?;
        Ok(Repo::new(backend, codec))
    }
}
