use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_data::backend::mem::MemBackend;
use airframe_data::cache::BackendByteCache;
use airframe_data::key::Key;
use airframe_secrets::{KeyResolver, SecretBytes, SecretCache};

struct StaticResolver {
    key: SecretBytes,
}
impl StaticResolver {
    fn new(key: SecretBytes) -> Self {
        Self { key }
    }
}
impl KeyResolver for StaticResolver {
    fn resolve(
        &self,
        _key_id: Option<&[u8]>,
    ) -> Result<SecretBytes, airframe_secrets::error::AirframeSecretsError> {
        Ok(self.key.expose(|k| SecretBytes::from_vec(k.to_vec())))
    }
}

#[test]
fn secret_cache_resolved_roundtrip_value() {
    let backend = MemBackend::new();
    let bytes = BackendByteCache::new(backend);
    let cache = SecretCache::new(bytes);

    let suite = SoftwareCipherSuite::new();
    let key = SecretBytes::from_vec(vec![3u8; 32]);
    let resolver = StaticResolver::new(key);

    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq, Debug)]
    struct Demo {
        a: u32,
        b: String,
    }

    let k = Key::new("demo:resolved").unwrap();
    let v = Demo {
        a: 7,
        b: "seven".into(),
    };

    cache
        .put_value_resolved(
            &k,
            &suite,
            SymmetricAlgorithm::AesGcm,
            &resolver,
            None,
            &v,
            None,
        )
        .unwrap();
    let out: Demo = cache
        .get_value_resolved(&k, &suite, &resolver, None, None)
        .unwrap()
        .unwrap();
    assert_eq!(out, v);
}

#[test]
fn secret_cache_resolved_roundtrip_bytes() {
    let backend = MemBackend::new();
    let bytes = BackendByteCache::new(backend);
    let cache = SecretCache::new(bytes);

    let suite = SoftwareCipherSuite::new();
    let key = SecretBytes::from_vec(vec![4u8; 32]);
    let resolver = StaticResolver::new(key);

    let k = Key::new("raw:resolved").unwrap();
    let pt = SecretBytes::from_vec(Vec::from("resolver secrets"));

    cache
        .put_encrypted_bytes_resolved(
            &k,
            &suite,
            SymmetricAlgorithm::ChaCha20Poly1305,
            &resolver,
            None,
            &pt,
            None,
        )
        .unwrap();
    let out = cache
        .get_decrypted_bytes_resolved(&k, &suite, &resolver, None, None)
        .unwrap()
        .unwrap();
    assert_eq!(out, b"resolver secrets");
}
