use std::marker::PhantomData;
use std::sync::Arc;

use airframe_data::backend::KvBackend;
use airframe_data::codec::Codec;
use airframe_data::key::Key;
// kept for trait bounds; not used directly here
use serde_json::Value;

use crate::error::{AirframeSdataError, Result};
use crate::model::DataModel;
use crate::schema::SchemaRegistry;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Envelope<V> {
    schema: String,
    version: u32,
    data: V,
}

#[derive(Clone)]
pub struct TypedRepo<B: KvBackend, C: Codec, T: DataModel> {
    backend: B,
    codec: C,
    registry: Arc<SchemaRegistry>,
    _t: PhantomData<T>,
}

impl<B: KvBackend, C: Codec, T: DataModel> TypedRepo<B, C, T> {
    pub fn new(backend: B, codec: C, registry: Arc<SchemaRegistry>) -> Self {
        Self {
            backend,
            codec,
            registry,
            _t: PhantomData,
        }
    }

    fn encode_envelope(&self, value: &T) -> Result<Vec<u8>> {
        let env = Envelope {
            schema: T::SCHEMA_NAME.to_string(),
            version: T::SCHEMA_VERSION,
            data: value,
        };
        self.codec
            .encode(&env)
            .map_err(|e| AirframeSdataError::CodecError(format!("{:?}", e)))
    }

    fn decode_to_value(&self, bytes: &[u8]) -> Result<Envelope<Value>> {
        self.codec
            .decode::<Envelope<Value>>(bytes)
            .map_err(|e| AirframeSdataError::CodecError(format!("{:?}", e)))
    }

    pub fn put(&self, key: &Key, value: &T) -> Result<()> {
        value.validate()?;
        let bytes = self.encode_envelope(value)?;
        self.backend
            .put_bytes(key, &bytes)
            .map_err(|_| AirframeSdataError::InvalidState)
    }

    pub fn get(&self, key: &Key) -> Result<Option<T>> {
        let opt = self
            .backend
            .get_bytes(key)
            .map_err(|_| AirframeSdataError::InvalidState)?;
        let Some(bytes) = opt else {
            return Ok(None);
        };
        let mut env = self.decode_to_value(&bytes)?;
        if env.schema != T::SCHEMA_NAME {
            return Err(AirframeSdataError::MigrationError(format!(
                "schema mismatch: stored={}, expected={}",
                env.schema,
                T::SCHEMA_NAME
            )));
        }
        if env.version < T::SCHEMA_VERSION {
            env.data = self.registry.migrate_chain(
                &env.schema,
                env.version,
                T::SCHEMA_VERSION,
                env.data,
            )?;
            env.version = T::SCHEMA_VERSION;
        } else if env.version > T::SCHEMA_VERSION {
            return Err(AirframeSdataError::MigrationError(format!(
                "stored version {} is newer than expected {}",
                env.version,
                T::SCHEMA_VERSION
            )));
        }
        let t: T = serde_json::from_value(env.data)
            .map_err(|e| AirframeSdataError::CodecError(e.to_string()))?;
        t.validate()?;
        Ok(Some(t))
    }

    pub fn remove(&self, key: &Key) -> Result<()> {
        self.backend
            .remove(key)
            .map_err(|_| AirframeSdataError::InvalidState)
    }
    pub fn contains(&self, key: &Key) -> Result<bool> {
        self.backend
            .contains(key)
            .map_err(|_| AirframeSdataError::InvalidState)
    }
}

#[derive(Clone, Debug)]
pub struct KeySpace {
    prefix: String,
}

impl KeySpace {
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
        }
    }
    pub fn key(&self, id: &str) -> Result<Key> {
        // Key disallows '/', so use ':' separator
        airframe_data::key::Key::new(format!("{}:{}", self.prefix, id))
            .map_err(|_| AirframeSdataError::InvalidState)
    }
}
