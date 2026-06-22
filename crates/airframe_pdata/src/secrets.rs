use std::sync::Arc;

use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_secrets as secrets;

use crate::context::PContext;
use crate::policy::{AadPolicy, BasicAadPolicy, Compression};

// Re-export secrets factors for ergonomic use from airframe_pdata::secrets
pub use secrets::factors::{FactorInput, FactorPolicy, FactorsKeyResolver, KdfSpec};

/// Adapter to use an airframe_secrets::KeyResolver where a pdata::KeyResolver is required.
#[derive(Clone)]
pub struct SecretsKeyResolverAdapter {
    inner: Arc<dyn secrets::KeyResolver + Send + Sync>,
}

impl SecretsKeyResolverAdapter {
    pub fn new(inner: Arc<dyn secrets::KeyResolver + Send + Sync>) -> Self {
        Self { inner }
    }
}

impl crate::context::KeyResolver for SecretsKeyResolverAdapter {
    fn resolve(&self, key_id: Option<&[u8]>) -> crate::Result<secrets::SecretBytes> {
        self.inner
            .resolve(key_id)
            .map_err(|_| crate::error::AirframePdataError::InvalidState)
    }
}

/// Convenience builder to construct a PContext from a FactorPolicy and inputs using FactorsKeyResolver.
#[derive(Clone)]
pub struct FactorsContextBuilder {
    suite: SoftwareCipherSuite,
    alg: SymmetricAlgorithm,
    policy: FactorPolicy,
    inputs: Vec<FactorInput>,
    // Optional context customizations
    namespace: Option<String>,
    key_id: Option<Vec<u8>>,
    aad_policy: Arc<dyn AadPolicy>,
    compression: Compression,
}

impl FactorsContextBuilder {
    /// Start building a PContext backed by FactorsKeyResolver.
    pub fn new(
        suite: SoftwareCipherSuite,
        alg: SymmetricAlgorithm,
        policy: FactorPolicy,
        inputs: Vec<FactorInput>,
    ) -> Self {
        Self {
            suite,
            alg,
            policy,
            inputs,
            namespace: None,
            key_id: None,
            aad_policy: Arc::new(BasicAadPolicy),
            compression: Compression::default(),
        }
    }

    /// Set an optional namespace to bind into AAD.
    pub fn namespace(mut self, ns: impl Into<String>) -> Self {
        self.namespace = Some(ns.into());
        self
    }

    /// Set an optional key id hint.
    pub fn key_id(mut self, kid: impl Into<Vec<u8>>) -> Self {
        self.key_id = Some(kid.into());
        self
    }

    /// Override the AAD policy.
    pub fn aad_policy(mut self, policy: Arc<dyn AadPolicy>) -> Self {
        self.aad_policy = policy;
        self
    }

    /// Set the compression policy.
    pub fn compression(mut self, compression: Compression) -> Self {
        self.compression = compression;
        self
    }

    /// Build the concrete PContext with an internal adapter around FactorsKeyResolver.
    pub fn build(self) -> PContext<SecretsKeyResolverAdapter> {
        let resolver = FactorsKeyResolver::new(self.policy, self.inputs);
        let adapter = SecretsKeyResolverAdapter::new(Arc::new(resolver));
        let mut ctx = PContext::new(self.suite, self.alg, adapter).with_aad_policy(self.aad_policy);
        ctx.namespace = self.namespace;
        ctx.key_id = self.key_id;
        ctx.compression = self.compression;
        ctx
    }
}

/// Minimal convenience function when defaults are fine (basic AAD, no namespace, no key_id, no compression).
pub fn context_from_factors(
    suite: SoftwareCipherSuite,
    alg: SymmetricAlgorithm,
    policy: FactorPolicy,
    inputs: Vec<FactorInput>,
) -> PContext<SecretsKeyResolverAdapter> {
    FactorsContextBuilder::new(suite, alg, policy, inputs).build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::KeyResolver; // bring trait into scope for method resolution
    use crate::policy::IndexedAadPolicy;
    use airframe_crypt::AlgorithmId;
    use airframe_data::key::Key;
    use airframe_secrets::error::AirframeSecretsError;
    use airframe_secrets::factors::FactorKind;
    use airframe_secrets::resolver::KeyResolver as SecretsResolver;
    use airframe_secrets::secret::SecretBytes as SecretsSecretBytes;
    use secrecy::SecretString;

    struct OkResolver;
    impl SecretsResolver for OkResolver {
        fn resolve(
            &self,
            _key_id: Option<&[u8]>,
        ) -> Result<SecretsSecretBytes, AirframeSecretsError> {
            Ok(SecretsSecretBytes::from_vec(vec![1, 2, 3, 4]))
        }
    }

    struct ErrResolver;
    impl SecretsResolver for ErrResolver {
        fn resolve(
            &self,
            _key_id: Option<&[u8]>,
        ) -> Result<SecretsSecretBytes, AirframeSecretsError> {
            Err(AirframeSecretsError::InvalidState)
        }
    }

    #[test]
    fn adapter_maps_ok_and_err() {
        let ok = SecretsKeyResolverAdapter::new(Arc::new(OkResolver));
        let got = ok.resolve(Some(b"kid"));
        assert!(got.is_ok());

        let err = SecretsKeyResolverAdapter::new(Arc::new(ErrResolver));
        let got = err.resolve(None);
        // Any error maps to pdata::InvalidState
        match got {
            Err(crate::error::AirframePdataError::InvalidState) => {}
            other => panic!("unexpected result: {:?}", other),
        }
    }

    #[test]
    fn factors_context_builder_sets_fields_and_derives_key() {
        // Build a simple policy requiring one factor; PBKDF2-SHA256
        let policy = FactorPolicy {
            kdf: KdfSpec {
                alg: AlgorithmId::Pbkdf2Sha256,
                iters: 100_000,
                salt: Some(vec![9, 9]),
            },
            min_factors: 1,
            domain: Some("test-domain".into()),
        };
        let inputs = vec![FactorInput {
            kind: FactorKind::Password,
            value: SecretString::new("pw".into()),
        }];
        let suite = SoftwareCipherSuite::new();
        let alg = SymmetricAlgorithm::AesGcm;

        // Use a non-default AAD policy to verify propagation
        let aad = Arc::new(IndexedAadPolicy::new(true)) as Arc<dyn AadPolicy>;
        let ctx = FactorsContextBuilder::new(suite, alg, policy.clone(), inputs.clone())
            .namespace("ns1")
            .key_id(b"kid1".to_vec())
            .aad_policy(aad.clone())
            .build();

        // Verify namespace and key_id propagated
        assert_eq!(ctx.namespace.as_deref(), Some("ns1"));
        assert_eq!(ctx.key_id.as_deref(), Some(&b"kid1"[..]));

        // AAD should be composed by our IndexedAadPolicy and include the key
        let k = Key::new("logical").unwrap();
        let aad_bytes = ctx.aad_for(&k, Some(b"meta"));
        let aad_str = String::from_utf8_lossy(&aad_bytes);
        assert!(aad_str.contains("ns=ns1|k=logical"));

        // Derive a key successfully via FactorsKeyResolver
        let key = ctx.resolve_key().expect("derived key");
        assert!(!key.is_empty());
    }

    #[test]
    fn minimal_context_from_factors_works() {
        let policy = FactorPolicy {
            kdf: KdfSpec {
                alg: AlgorithmId::Pbkdf2Sha256,
                iters: 100_000,
                salt: None,
            },
            min_factors: 1,
            domain: None,
        };
        let inputs = vec![FactorInput {
            kind: FactorKind::Data,
            value: SecretString::new("abc".into()),
        }];
        let ctx = context_from_factors(
            SoftwareCipherSuite::new(),
            SymmetricAlgorithm::AesGcm,
            policy,
            inputs,
        );
        let k = ctx.resolve_key().expect("key");
        assert!(k.len() >= 16);
    }
}
