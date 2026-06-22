use crate::error::Result;
use crate::policy::{AadPolicy, BasicAadPolicy, Compression};
use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_data::key::Key;
use airframe_secrets::secret::SecretBytes;
use std::sync::Arc;

/// A trait to resolve an encryption key, potentially by key-id.
pub trait KeyResolver: Send + Sync + 'static {
    fn resolve(&self, key_id: Option<&[u8]>) -> Result<SecretBytes>;
}

/// Context for protected storage operations.
#[derive(Clone)]
pub struct PContext<R: KeyResolver> {
    pub suite: SoftwareCipherSuite,
    pub alg: SymmetricAlgorithm,
    pub aad_policy: Arc<dyn AadPolicy>,
    pub resolver: R,
    pub key_id: Option<Vec<u8>>, // optional KID
    pub namespace: Option<String>,
    pub compression: Compression,
}

impl<R: KeyResolver> PContext<R> {
    pub fn new(suite: SoftwareCipherSuite, alg: SymmetricAlgorithm, resolver: R) -> Self {
        Self {
            suite,
            alg,
            aad_policy: Arc::new(BasicAadPolicy),
            resolver,
            key_id: None,
            namespace: None,
            compression: Compression::default(),
        }
    }

    /// Override the AAD policy used to compose AEAD associated data.
    pub fn with_aad_policy(mut self, policy: Arc<dyn AadPolicy>) -> Self {
        self.aad_policy = policy;
        self
    }

    /// Compute AAD for a given Key, with optional extra context bytes.
    pub fn aad_for(&self, key: &Key, extra: Option<&[u8]>) -> Vec<u8> {
        let ns = self.namespace.as_deref().unwrap_or("");
        self.aad_policy.compose(ns, key, extra)
    }

    /// Resolve the encryption key using the configured resolver and key_id.
    pub fn resolve_key(&self) -> Result<SecretBytes> {
        self.resolver.resolve(self.key_id.as_deref())
    }
}
