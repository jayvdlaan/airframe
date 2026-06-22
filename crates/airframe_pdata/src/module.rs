use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tracing::info;

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_CRYPT, CAP_PDATA};
use airframe_core::registry::ServiceRegistry;
use airframe_macros::module_descriptor;

use airframe_data::backend::fs_secure::FsBackendSecure;
use airframe_data::backend::mem::MemBackend;
use airframe_data::cache::BackendByteCache;
use airframe_data::codec::{BincodeCodec, JsonCodec};
use std::path::Path;

use airframe_crypt::suite::{CipherSuite, SoftwareCipherSuite};
use airframe_crypt::sym::SymmetricAlgorithm;

use crate::builder::PDataBuilder;
use crate::bytes::PStoreBytes;
use crate::context::{KeyResolver as PKeyResolver, PContext};
use crate::typed::PStore;

/// Adapter: allow using airframe_secrets::KeyResolver where a pdata::KeyResolver is needed.
#[derive(Clone)]
pub struct SecretsKeyResolverAdapter {
    inner: Arc<dyn airframe_secrets::KeyResolver + Send + Sync>,
}

impl SecretsKeyResolverAdapter {
    pub fn new(inner: Arc<dyn airframe_secrets::KeyResolver + Send + Sync>) -> Self {
        Self { inner }
    }
}

impl PKeyResolver for SecretsKeyResolverAdapter {
    fn resolve(&self, key_id: Option<&[u8]>) -> crate::Result<airframe_secrets::SecretBytes> {
        self.inner
            .resolve(key_id)
            .map_err(|_e| crate::error::AirframePdataError::InvalidState)
    }
}

/// A thin factory registered by PDataModule to help construct common pdata stacks.
///
/// This keeps the module reusable while avoiding exposing generics over trait objects.
#[derive(Clone)]
pub struct PDataFactory {
    suite: Arc<SoftwareCipherSuite>,
}

impl PDataFactory {
    pub fn new(suite: Arc<SoftwareCipherSuite>) -> Self {
        Self { suite }
    }

    /// Access the software cipher suite (from crypt capability).
    pub fn suite(&self) -> Arc<SoftwareCipherSuite> {
        self.suite.clone()
    }

    /// Build a PContext using a secrets::KeyResolver via adapter.
    pub fn context_with_secrets(
        &self,
        alg: SymmetricAlgorithm,
        resolver: Arc<dyn airframe_secrets::KeyResolver + Send + Sync>,
    ) -> PContext<SecretsKeyResolverAdapter> {
        let adapter = SecretsKeyResolverAdapter::new(resolver);
        PContext::new(*self.suite.as_ref(), alg, adapter)
    }

    /// Build a bytes store over an in-memory backend using the provided context.
    pub fn bytes_mem<R: PKeyResolver>(
        &self,
        ctx: PContext<R>,
    ) -> PStoreBytes<BackendByteCache<MemBackend>, R> {
        let backend = MemBackend::new();
        let bc = BackendByteCache::new(backend);
        PDataBuilder::new()
            .bytes(bc)
            .context(ctx)
            .build_bytes()
            .expect("valid pdata bytes")
    }

    /// Build a typed store with JsonCodec over in-memory backend using the provided context.
    pub fn typed_json_mem<R: PKeyResolver>(
        &self,
        ctx: PContext<R>,
    ) -> PStore<JsonCodec, BackendByteCache<MemBackend>, R> {
        let backend = MemBackend::new();
        let bc = BackendByteCache::new(backend);
        let codec = JsonCodec;
        PDataBuilder::new()
            .bytes(bc)
            .context(ctx)
            .build_typed(codec)
            .expect("valid pdata typed")
    }

    /// Build a bytes store over FsBackendSecure using the provided context.
    pub fn bytes_fs_secure<R: PKeyResolver, P: AsRef<Path>>(
        &self,
        ctx: PContext<R>,
        root: P,
        ext: &str,
    ) -> PStoreBytes<BackendByteCache<FsBackendSecure>, R> {
        let backend = FsBackendSecure::new(root, ext).expect("FsBackendSecure");
        let bc = BackendByteCache::new(backend);
        PDataBuilder::new()
            .bytes(bc)
            .context(ctx)
            .build_bytes()
            .expect("valid pdata bytes")
    }

    /// Build a typed store with JsonCodec over FsBackendSecure using the provided context.
    pub fn typed_json_fs_secure<R: PKeyResolver, P: AsRef<Path>>(
        &self,
        ctx: PContext<R>,
        root: P,
        ext: &str,
    ) -> PStore<JsonCodec, BackendByteCache<FsBackendSecure>, R> {
        let backend = FsBackendSecure::new(root, ext).expect("FsBackendSecure");
        let bc = BackendByteCache::new(backend);
        let codec = JsonCodec;
        PDataBuilder::new()
            .bytes(bc)
            .context(ctx)
            .build_typed(codec)
            .expect("valid pdata typed")
    }

    /// Build a typed store with BincodeCodec over FsBackendSecure using the provided context.
    pub fn typed_bincode_fs_secure<R: PKeyResolver, P: AsRef<Path>>(
        &self,
        ctx: PContext<R>,
        root: P,
        ext: &str,
    ) -> PStore<BincodeCodec, BackendByteCache<FsBackendSecure>, R> {
        let backend = FsBackendSecure::new(root, ext).expect("FsBackendSecure");
        let bc = BackendByteCache::new(backend);
        let codec = BincodeCodec;
        PDataBuilder::new()
            .bytes(bc)
            .context(ctx)
            .build_typed(codec)
            .expect("valid pdata typed")
    }
}

/// Provides pdata by registering a PDataFactory built from the crypt capability.
///
/// Registered services:
/// - `Arc<PDataFactory>`
///
/// Requires crypt and can leverage secrets if present via adapter.
pub struct PDataModule {
    desc: ModuleDescriptor,
}

impl Default for PDataModule {
    fn default() -> Self {
        Self::new()
    }
}

impl PDataModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "pdata",
                version: "0.1.0",
                provides: [CAP_PDATA.0],
                requires: [CAP_CRYPT.0]
            ),
        }
    }
}

#[async_trait]
impl Module for PDataModule {
    airframe_macros::impl_descriptor!();

    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        // Assert crypt is present and get the concrete suite.
        let suite = ctx
            .services
            .get::<SoftwareCipherSuite>()
            .unwrap_or_else(|| {
                panic!("{} SoftwareCipherSuite must be present", CAP_CRYPT.as_str())
            });
        // Also ensure dyn CipherSuite present for dependency sanity (similar to CryptModule tests).
        let _dyn_suite = ctx
            .services
            .get::<dyn CipherSuite>()
            .unwrap_or_else(|| panic!("{} dyn CipherSuite must be present", CAP_CRYPT.as_str()));

        // Register factory.
        info!(target = "airframe_pdata", "PDataFactory registered");
        ctx.services
            .register::<PDataFactory>(Arc::new(PDataFactory::new(suite)));
        Ok(())
    }
}

/// Convenience accessor for pdata-related services.
pub trait ServiceRegistryPDataExt {
    fn pdata_factory(&self) -> Option<Arc<PDataFactory>>;
}

impl ServiceRegistryPDataExt for ServiceRegistry {
    fn pdata_factory(&self) -> Option<Arc<PDataFactory>> {
        self.get::<PDataFactory>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airframe_core::app::AppBuilder;
    use airframe_data::key::Key;

    struct StaticResolver;
    impl airframe_secrets::KeyResolver for StaticResolver {
        fn resolve(
            &self,
            _key_id: Option<&[u8]>,
        ) -> airframe_secrets::error::Result<airframe_secrets::SecretBytes> {
            Ok(airframe_secrets::SecretBytes::from_vec(vec![7u8; 32]))
        }
    }

    #[tokio::test]
    async fn registers_factory_and_roundtrip_bytes() {
        let app = AppBuilder::new()
            .with(airframe_crypt::CryptModule::new())
            .with(PDataModule::new())
            .start()
            .await
            .unwrap();

        let pd = app.services.pdata_factory().expect("PDataFactory present");
        let ctx = pd.context_with_secrets(
            SymmetricAlgorithm::ChaCha20Poly1305,
            Arc::new(StaticResolver),
        );
        let bytes = pd.bytes_mem(ctx.clone());
        let k = Key::new("demo:1").unwrap();
        bytes.put_bytes(&k, b"hello").unwrap();
        let out = bytes.get_bytes(&k).unwrap().unwrap();
        assert_eq!(&out, b"hello");
    }

    #[tokio::test]
    async fn registers_factory_and_roundtrip_typed_json() {
        let app = AppBuilder::new()
            .with(airframe_crypt::CryptModule::new())
            .with(PDataModule::new())
            .start()
            .await
            .unwrap();

        #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
        struct Demo {
            a: u32,
            b: String,
        }

        let pd = app.services.pdata_factory().expect("PDataFactory present");
        let ctx = pd.context_with_secrets(SymmetricAlgorithm::AesGcm, Arc::new(StaticResolver));
        let store = pd.typed_json_mem(ctx.clone());
        let k = Key::new("demo:2").unwrap();
        let v = Demo {
            a: 42,
            b: "x".into(),
        };
        store.put(&k, &v).unwrap();
        let out: Demo = store.get(&k).unwrap().unwrap();
        assert_eq!(out, v);
    }
}
