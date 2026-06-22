use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tracing::info;

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_SDATA};
use airframe_core::registry::ServiceRegistry;
use airframe_macros::module_descriptor;

use airframe_data::backend::mem::MemBackend;
use airframe_data::cache::BackendByteCache;
use airframe_data::codec::JsonCodec;

use crate::cache::SchemaCache;
use crate::model::DataModel;
use crate::schema::SchemaRegistry;
use crate::store::TypedRepo;

#[cfg(any(feature = "integration-pdata", feature = "protected"))]
use crate::cache::ProtectedSchemaCache;
#[cfg(any(feature = "integration-pdata", feature = "protected"))]
use crate::protected::ProtectedTypedRepo;
#[cfg(any(feature = "integration-pdata", feature = "protected"))]
use airframe_pdata::context::{KeyResolver as PKeyResolver, PContext};

/// Factory registered by SDataModule to build common sdata primitives.
#[derive(Clone)]
pub struct SDataFactory;
impl Default for SDataFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl SDataFactory {
    pub fn new() -> Self {
        Self
    }

    /// In-memory typed repository using JsonCodec.
    pub fn typed_json_mem<T: DataModel>(
        &self,
        registry: Arc<SchemaRegistry>,
    ) -> TypedRepo<MemBackend, JsonCodec, T> {
        let backend = MemBackend::new();
        let codec = JsonCodec;
        TypedRepo::new(backend, codec, registry)
    }

    /// In-memory schema-aware cache using `BackendByteCache<MemBackend>` and JsonCodec.
    pub fn schema_cache_json_mem<T: DataModel>(
        &self,
        registry: Arc<SchemaRegistry>,
    ) -> SchemaCache<JsonCodec, BackendByteCache<MemBackend>, T> {
        let bytes = BackendByteCache::new(MemBackend::new());
        let codec = JsonCodec;
        SchemaCache::new(codec, bytes, registry)
    }

    /// Protected typed repo over in-memory BackendByteCache<MemBackend> using pdata context.
    #[cfg(any(feature = "integration-pdata", feature = "protected"))]
    pub fn protected_repo_json_mem<T: DataModel, R: PKeyResolver>(
        &self,
        ctx: PContext<R>,
        registry: Arc<SchemaRegistry>,
    ) -> ProtectedTypedRepo<JsonCodec, BackendByteCache<MemBackend>, R, T> {
        let bc = BackendByteCache::new(MemBackend::new());
        let pbytes = airframe_pdata::bytes::PStoreBytes::new(bc, ctx);
        ProtectedTypedRepo::new(JsonCodec, pbytes, registry)
    }

    /// Protected schema cache over in-memory BackendByteCache<MemBackend> using pdata context.
    #[cfg(any(feature = "integration-pdata", feature = "protected"))]
    pub fn protected_schema_cache_json_mem<T: DataModel, R: PKeyResolver>(
        &self,
        ctx: PContext<R>,
        registry: Arc<SchemaRegistry>,
    ) -> ProtectedSchemaCache<JsonCodec, BackendByteCache<MemBackend>, R, T> {
        let bc = BackendByteCache::new(MemBackend::new());
        let pbytes = airframe_pdata::bytes::PStoreBytes::new(bc, ctx);
        ProtectedSchemaCache::new(JsonCodec, pbytes, registry)
    }
}

/// SData module that registers SDataFactory into the service registry.
pub struct SDataModule {
    desc: ModuleDescriptor,
}
impl Default for SDataModule {
    fn default() -> Self {
        Self::new()
    }
}

impl SDataModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "sdata",
                version: "0.1.0",
                provides: [CAP_SDATA.0]
            ),
        }
    }
}

#[async_trait]
impl Module for SDataModule {
    airframe_macros::impl_descriptor!();
    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        info!(target = "airframe_sdata", "SDataFactory registered");
        ctx.services
            .register::<SDataFactory>(Arc::new(SDataFactory::new()));
        Ok(())
    }
}

/// Convenience accessor for SData services.
pub trait ServiceRegistrySDataExt {
    fn sdata_factory(&self) -> Option<Arc<SDataFactory>>;
}
impl ServiceRegistrySDataExt for ServiceRegistry {
    fn sdata_factory(&self) -> Option<Arc<SDataFactory>> {
        self.get::<SDataFactory>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DataModel;
    use airframe_core::module::CAP_SDATA;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Serialize, Deserialize)]
    struct DummyModel {
        a: u32,
    }
    impl DataModel for DummyModel {
        const SCHEMA_NAME: &'static str = "dummy";
        const SCHEMA_VERSION: u32 = 1;
    }

    #[test]
    fn descriptor_and_registration() {
        // Check descriptor values
        let m = SDataModule::new();
        let desc = m.descriptor();
        assert_eq!(desc.name, "sdata");
        assert!(desc.provides.contains(&CAP_SDATA.0));
        // The module avoids upward edges; optional_requires should be empty.
        assert!(desc.optional_requires.is_empty());
        // We can't call async init without bringing runtime; we still can ensure
        // that the factory itself constructs repos and caches, and that the
        // extension method compiles and resolves to None on empty registry.
        let reg = ServiceRegistry::default();
        assert!(reg.sdata_factory().is_none());
    }

    #[test]
    fn factory_builders() {
        // Ensure factory methods construct types without panic
        let factory = SDataFactory::new();
        let registry = Arc::new(SchemaRegistry::new());

        // typed repo and schema cache basic construction
        let _repo = factory.typed_json_mem::<DummyModel>(registry.clone());
        let _cache = factory.schema_cache_json_mem::<DummyModel>(registry.clone());
    }
}
