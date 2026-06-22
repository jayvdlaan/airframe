use std::marker::PhantomData;
use std::sync::Arc;

use airframe_data::cache::ByteCache;
use airframe_data::codec::Codec;
use airframe_data::key::Key;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

#[cfg(any(feature = "integration-pdata", feature = "protected"))]
use airframe_data::backend::fs_secure::FsBackendSecure;
#[cfg(any(feature = "integration-pdata", feature = "protected"))]
use airframe_data::cache::BackendByteCache;

use crate::error::{AirframeSdataError, Result};
use crate::model::DataModel;
use crate::schema::SchemaRegistry;

#[cfg(any(feature = "integration-pdata", feature = "protected"))]
use airframe_pdata::bytes::PStoreBytes;
#[cfg(any(feature = "integration-pdata", feature = "protected"))]
use airframe_pdata::context::KeyResolver as PKeyResolver;
#[cfg(any(feature = "integration-pdata", feature = "protected"))]
use airframe_pdata::context::PContext;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Envelope<V> {
    schema: String,
    version: u32,
    data: V,
}

/// Protected typed repository: schema-aware, validated, migratable, and protected-at-rest via pdata.
#[cfg(any(feature = "integration-pdata", feature = "protected"))]
#[derive(Clone)]
pub struct ProtectedTypedRepo<C: Codec, BC: ByteCache, R: PKeyResolver, T: DataModel> {
    codec: C,
    pbytes: PStoreBytes<BC, R>,
    registry: Arc<SchemaRegistry>,
    _t: PhantomData<T>,
}

#[cfg(any(feature = "integration-pdata", feature = "protected"))]
impl<C: Codec, BC: ByteCache, R: PKeyResolver, T: DataModel> ProtectedTypedRepo<C, BC, R, T> {
    pub fn new(codec: C, pbytes: PStoreBytes<BC, R>, registry: Arc<SchemaRegistry>) -> Self {
        Self {
            codec,
            pbytes,
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
        self.pbytes
            .put_bytes(key, &bytes)
            .map_err(|_| AirframeSdataError::InvalidState)
    }

    pub fn get(&self, key: &Key) -> Result<Option<T>> {
        let opt = self
            .pbytes
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
        self.pbytes
            .remove(key)
            .map_err(|_| AirframeSdataError::InvalidState)
    }

    pub fn contains(&self, key: &Key) -> Result<bool> {
        self.pbytes
            .contains(key)
            .map_err(|_| AirframeSdataError::InvalidState)
    }

    /// List stored keys
    pub fn list(&self) -> Result<Vec<Key>> {
        self.pbytes
            .list()
            .map_err(|_| AirframeSdataError::InvalidState)
    }

    /// Rotate encryption to a new context
    pub fn rewrap<R2: PKeyResolver>(&self, key: &Key, new_ctx: &PContext<R2>) -> Result<()> {
        let ok = self
            .pbytes
            .rewrap_to(key, new_ctx)
            .map_err(|_| AirframeSdataError::InvalidState)?;
        if ok {
            Ok(())
        } else {
            Err(AirframeSdataError::InvalidState)
        }
    }

    /// Put while binding AAD to clear index bytes
    pub fn put_with_index(&self, key: &Key, value: &T, clear_index_bytes: &[u8]) -> Result<()> {
        value.validate()?;
        let bytes = self.encode_envelope(value)?;
        self.pbytes
            .put_bytes_with_meta(key, &bytes, Some(clear_index_bytes))
            .map_err(|_| AirframeSdataError::InvalidState)
    }

    /// Get while providing index bytes to satisfy AAD binding
    pub fn get_with_index(&self, key: &Key, clear_index_bytes: &[u8]) -> Result<Option<T>> {
        let opt = self
            .pbytes
            .get_bytes_with_meta(key, Some(clear_index_bytes))
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
}

/// Ergonomic builder to assemble a protected typed repo.
#[cfg(any(feature = "integration-pdata", feature = "protected"))]
#[derive(Clone)]
pub struct SDataProtectedBuilder<BC: ByteCache, R: PKeyResolver> {
    bytes: Option<BC>,
    context: Option<PContext<R>>,
    registry: Option<Arc<SchemaRegistry>>,
}

#[cfg(any(feature = "integration-pdata", feature = "protected"))]
impl<BC: ByteCache, R: PKeyResolver> SDataProtectedBuilder<BC, R> {
    pub fn new() -> Self {
        Self {
            bytes: None,
            context: None,
            registry: None,
        }
    }
    pub fn bytes(mut self, bc: BC) -> Self {
        self.bytes = Some(bc);
        self
    }
    pub fn context(mut self, ctx: PContext<R>) -> Self {
        self.context = Some(ctx);
        self
    }
    pub fn registry(mut self, registry: Arc<SchemaRegistry>) -> Self {
        self.registry = Some(registry);
        self
    }

    pub fn build_typed<C: Codec, T: DataModel>(
        self,
        codec: C,
    ) -> Result<ProtectedTypedRepo<C, BC, R, T>> {
        let bc = self.bytes.ok_or(AirframeSdataError::InvalidState)?;
        let ctx = self.context.ok_or(AirframeSdataError::InvalidState)?;
        let reg = self.registry.ok_or(AirframeSdataError::InvalidState)?;
        let pbytes = PStoreBytes::new(bc, ctx);
        Ok(ProtectedTypedRepo::new(codec, pbytes, reg))
    }
}

#[cfg(any(feature = "integration-pdata", feature = "protected"))]
#[derive(Clone)]
pub struct SDataProtectedFsBuilder<R: PKeyResolver> {
    root: std::path::PathBuf,
    ext: String,
    ctx: Option<PContext<R>>,
    registry: Option<Arc<SchemaRegistry>>,
}

#[cfg(any(feature = "integration-pdata", feature = "protected"))]
impl<R: PKeyResolver> SDataProtectedFsBuilder<R> {
    pub fn new<P: AsRef<std::path::Path>>(root: P, ext: &str) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            ext: ext.trim_start_matches('.').to_string(),
            ctx: None,
            registry: None,
        }
    }
    pub fn context(mut self, ctx: PContext<R>) -> Self {
        self.ctx = Some(ctx);
        self
    }
    pub fn registry(mut self, reg: Arc<SchemaRegistry>) -> Self {
        self.registry = Some(reg);
        self
    }

    pub fn build_typed<C: Codec, T: DataModel>(
        self,
        codec: C,
    ) -> Result<ProtectedTypedRepo<C, BackendByteCache<FsBackendSecure>, R, T>> {
        let ctx = self.ctx.ok_or(AirframeSdataError::InvalidState)?;
        let reg = self.registry.ok_or(AirframeSdataError::InvalidState)?;
        let backend = FsBackendSecure::new(&self.root, &self.ext)
            .map_err(|_| AirframeSdataError::InvalidState)?;
        let bc = BackendByteCache::new(backend);
        let pbytes = PStoreBytes::new(bc, ctx);
        Ok(ProtectedTypedRepo::new(codec, pbytes, reg))
    }
}
