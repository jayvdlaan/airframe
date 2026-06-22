use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_CRYPT};
use airframe_core::registry::ServiceRegistry;
use airframe_macros::module_descriptor;
use tracing::info;

use crate::suite::{CipherSuite, SoftwareCipherSuite};

/// Capability provider for cryptographic operations.
///
/// Registers a CipherSuite into the ServiceRegistry so other modules can discover
/// and use cryptographic operations uniformly.
///
/// Registered services:
/// - `Arc<dyn CipherSuite>`
/// - `Arc<SoftwareCipherSuite>` (concrete convenience)
pub struct CryptModule {
    desc: ModuleDescriptor,
}

impl Default for CryptModule {
    fn default() -> Self {
        Self::new()
    }
}

impl CryptModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "crypt",
                version: "0.1.0",
                provides: [CAP_CRYPT.0]
            ),
        }
    }
}

#[async_trait]
impl Module for CryptModule {
    airframe_macros::impl_descriptor!();

    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        info!(target = "airframe_crypt", "registering crypt suite");
        // For now, wire the default software-backed suite.
        let suite = Arc::new(SoftwareCipherSuite::new());
        // Register concrete for consumers that want to downcast
        ctx.services.register::<SoftwareCipherSuite>(suite.clone());
        // Register trait object for backend-agnostic use
        let dyn_suite: Arc<dyn CipherSuite> = suite;
        ctx.services.register::<dyn CipherSuite>(dyn_suite);
        Ok(())
    }
}

/// Convenience accessors to retrieve the crypt suite(s) from the ServiceRegistry.
pub trait ServiceRegistryCryptExt {
    fn crypt(&self) -> Option<Arc<dyn CipherSuite>>;
    fn crypt_software(&self) -> Option<Arc<SoftwareCipherSuite>>;
}

impl ServiceRegistryCryptExt for ServiceRegistry {
    fn crypt(&self) -> Option<Arc<dyn CipherSuite>> {
        self.get::<dyn CipherSuite>()
    }
    fn crypt_software(&self) -> Option<Arc<SoftwareCipherSuite>> {
        self.get::<SoftwareCipherSuite>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airframe_core::app::AppBuilder;

    #[tokio::test]
    async fn registers_cipher_suite() {
        let app = AppBuilder::new()
            .with(CryptModule::new())
            .start()
            .await
            .unwrap();
        // Dyn trait path
        let suite = app
            .services
            .get::<dyn CipherSuite>()
            .expect("suite present");
        // Basic operation: random + digest roundtrip
        let key = suite.random_bytes(32).unwrap();
        assert_eq!(key.len(), 32);
        let hash = suite
            .digest(crate::hash::DigestAlgorithm::Sha256, b"abc")
            .unwrap();
        assert!(!hash.is_empty());
        // Concrete path
        let sw = app
            .services
            .get::<SoftwareCipherSuite>()
            .expect("concrete present");
        let nonce = sw.random_bytes(12).unwrap();
        let ct = sw
            .sym_encrypt(
                crate::sym::SymmetricAlgorithm::AesGcm,
                &key,
                &nonce,
                b"hello",
                None,
            )
            .unwrap();
        let pt = sw
            .sym_decrypt(
                crate::sym::SymmetricAlgorithm::AesGcm,
                &key,
                &nonce,
                &ct,
                None,
            )
            .unwrap();
        assert_eq!(&pt, b"hello");
    }
}
