use std::marker::PhantomData;
use std::sync::Arc;

use airframe_data::cache::ByteCache;
use airframe_data::codec::Codec;
use airframe_data::key::Key;
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

/// Schema-aware, validated, migratable typed cache on top of a ByteCache.
#[derive(Clone)]
pub struct SchemaCache<C: Codec, BC: ByteCache, T: DataModel> {
    codec: C,
    bytes: BC,
    registry: Arc<SchemaRegistry>,
    _t: PhantomData<T>,
}

impl<C: Codec, BC: ByteCache, T: DataModel> SchemaCache<C, BC, T> {
    pub fn new(codec: C, bytes: BC, registry: Arc<SchemaRegistry>) -> Self {
        Self {
            codec,
            bytes,
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
        self.bytes
            .put_bytes(key, &bytes)
            .map_err(|_| AirframeSdataError::InvalidState)
    }

    pub fn get(&self, key: &Key) -> Result<Option<T>> {
        let opt = self
            .bytes
            .get_bytes(key)
            .map_err(|_| AirframeSdataError::InvalidState)?;
        let Some(bytes) = opt else { return Ok(None) };
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
        self.bytes
            .remove(key)
            .map_err(|_| AirframeSdataError::InvalidState)
    }
    pub fn contains(&self, key: &Key) -> Result<bool> {
        self.bytes
            .contains(key)
            .map_err(|_| AirframeSdataError::InvalidState)
    }
    pub fn list(&self) -> Result<Vec<Key>> {
        self.bytes
            .list()
            .map_err(|_| AirframeSdataError::InvalidState)
    }
}

/// Builder for SchemaCache to hide generics at the call-site.
#[derive(Clone)]
pub struct SDataCacheBuilder<BC: ByteCache> {
    bytes: Option<BC>,
    registry: Option<Arc<SchemaRegistry>>,
}

impl<BC: ByteCache> Default for SDataCacheBuilder<BC> {
    fn default() -> Self {
        Self::new()
    }
}

impl<BC: ByteCache> SDataCacheBuilder<BC> {
    pub fn new() -> Self {
        Self {
            bytes: None,
            registry: None,
        }
    }
    pub fn bytes(mut self, bc: BC) -> Self {
        self.bytes = Some(bc);
        self
    }
    pub fn registry(mut self, reg: Arc<SchemaRegistry>) -> Self {
        self.registry = Some(reg);
        self
    }
    pub fn build_typed<C: Codec, T: DataModel>(self, codec: C) -> Result<SchemaCache<C, BC, T>> {
        let bc = self.bytes.ok_or(AirframeSdataError::InvalidState)?;
        let reg = self.registry.ok_or(AirframeSdataError::InvalidState)?;
        Ok(SchemaCache::new(codec, bc, reg))
    }
}

#[cfg(any(feature = "integration-pdata", feature = "protected"))]
mod protected_cache {
    use super::*;
    use airframe_pdata::bytes::PStoreBytes;
    use airframe_pdata::context::{KeyResolver, PContext};

    /// Protected schema-aware cache over PStoreBytes (uses CtE pipeline from pdata).
    #[derive(Clone)]
    pub struct ProtectedSchemaCache<C: Codec, BC: ByteCache, R: KeyResolver, T: DataModel> {
        codec: C,
        pbytes: PStoreBytes<BC, R>,
        registry: Arc<SchemaRegistry>,
        _t: PhantomData<T>,
    }

    impl<C: Codec, BC: ByteCache, R: KeyResolver, T: DataModel> ProtectedSchemaCache<C, BC, R, T> {
        pub fn new(codec: C, pbytes: PStoreBytes<BC, R>, registry: Arc<SchemaRegistry>) -> Self {
            Self {
                codec,
                pbytes,
                registry,
                _t: PhantomData,
            }
        }
        fn encode_env(&self, value: &T) -> Result<Vec<u8>> {
            let env = Envelope {
                schema: T::SCHEMA_NAME.to_string(),
                version: T::SCHEMA_VERSION,
                data: value,
            };
            self.codec
                .encode(&env)
                .map_err(|e| AirframeSdataError::CodecError(format!("{:?}", e)))
        }
        fn decode_env(&self, bytes: &[u8]) -> Result<Envelope<Value>> {
            self.codec
                .decode::<Envelope<Value>>(bytes)
                .map_err(|e| AirframeSdataError::CodecError(format!("{:?}", e)))
        }
        pub fn put(&self, key: &Key, value: &T) -> Result<()> {
            value.validate()?;
            let bytes = self.encode_env(value)?;
            self.pbytes
                .put_bytes(key, &bytes)
                .map_err(|_| AirframeSdataError::InvalidState)
        }
        pub fn get(&self, key: &Key) -> Result<Option<T>> {
            let Some(bytes) = self
                .pbytes
                .get_bytes(key)
                .map_err(|_| AirframeSdataError::InvalidState)?
            else {
                return Ok(None);
            };
            let mut env = self.decode_env(&bytes)?;
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
        pub fn list(&self) -> Result<Vec<Key>> {
            self.pbytes
                .list()
                .map_err(|_| AirframeSdataError::InvalidState)
        }
    }

    /// Builder for ProtectedSchemaCache
    #[derive(Clone)]
    pub struct SDataProtectedCacheBuilder<BC: ByteCache, R: KeyResolver> {
        bytes: Option<BC>,
        context: Option<PContext<R>>,
        registry: Option<Arc<SchemaRegistry>>,
    }

    impl<BC: ByteCache, R: KeyResolver> SDataProtectedCacheBuilder<BC, R> {
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
        pub fn registry(mut self, reg: Arc<SchemaRegistry>) -> Self {
            self.registry = Some(reg);
            self
        }
        pub fn build_typed<C: Codec, T: DataModel>(
            self,
            codec: C,
        ) -> Result<ProtectedSchemaCache<C, BC, R, T>> {
            let bc = self.bytes.ok_or(AirframeSdataError::InvalidState)?;
            let ctx = self.context.ok_or(AirframeSdataError::InvalidState)?;
            let reg = self.registry.ok_or(AirframeSdataError::InvalidState)?;
            let pbytes = airframe_pdata::bytes::PStoreBytes::new(bc, ctx);
            Ok(ProtectedSchemaCache::new(codec, pbytes, reg))
        }
    }

    pub use ProtectedSchemaCache as Protected;
    pub use SDataProtectedCacheBuilder as Builder;
}

#[cfg(any(feature = "integration-pdata", feature = "protected"))]
pub use protected_cache::{
    Builder as SDataProtectedCacheBuilder, Protected as ProtectedSchemaCache,
};

#[cfg(test)]
mod tests {
    use super::*;
    use airframe_data::backend::mem::MemBackend;
    use airframe_data::cache::BackendByteCache;
    use airframe_data::codec::JsonCodec;

    use crate::model::DataModel;
    use crate::schema::{Migrator, SchemaRegistry};
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct UserV2 {
        id: String,
        name: String,
        age: u32,
    }
    impl DataModel for UserV2 {
        const SCHEMA_NAME: &'static str = "user";
        const SCHEMA_VERSION: u32 = 2;
    }

    struct UserMigV1toV2;
    impl Migrator for UserMigV1toV2 {
        fn schema_name(&self) -> &'static str {
            "user"
        }
        fn migrate(
            &self,
            _from: u32,
            _to: u32,
            mut v: serde_json::Value,
        ) -> crate::error::Result<serde_json::Value> {
            if let Some(obj) = v.as_object_mut() {
                obj.entry("age").or_insert(json!(0));
            }
            Ok(v)
        }
    }

    #[test]
    fn schema_cache_roundtrip_with_migration() {
        let backend = MemBackend::new();
        let bytes = BackendByteCache::new(backend);
        let codec = JsonCodec;
        let mut reg = SchemaRegistry::new();
        reg.register_step("user", 1, Arc::new(UserMigV1toV2));
        let reg = Arc::new(reg);
        let cache: SchemaCache<_, _, UserV2> = SchemaCache::new(codec, bytes, reg.clone());
        let key = Key::new("alice").unwrap();

        // simulate legacy v1 stored raw bytes
        #[derive(Serialize)]
        struct Legacy {
            id: String,
            name: String,
        }
        let legacy = Legacy {
            id: "alice".into(),
            name: "Alice".into(),
        };
        #[derive(Serialize)]
        struct Env<'a, T> {
            schema: &'a str,
            version: u32,
            data: &'a T,
        }
        let env = Env {
            schema: "user",
            version: 1,
            data: &legacy,
        };
        let legacy_bytes = JsonCodec.encode(&env).unwrap();
        cache.bytes.put_bytes(&key, &legacy_bytes).unwrap();

        let out = cache.get(&key).unwrap().unwrap();
        assert_eq!(
            out,
            UserV2 {
                id: "alice".into(),
                name: "Alice".into(),
                age: 0
            }
        );
    }
}
