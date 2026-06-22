use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_data::backend::mem::MemBackend;
use airframe_data::cache::BackendByteCache;
use airframe_data::cache::ByteCache;
use airframe_data::key::Key;
use airframe_secrets::{SecretBytes, SecretCache};

// A failing ByteCache to simulate permission/IO errors
#[derive(Clone)]
struct FailingCache;
impl ByteCache for FailingCache {
    fn put_bytes(&self, _key: &Key, _bytes: &[u8]) -> airframe_data::error::Result<()> {
        Err(airframe_data::error::AirframeDataError::InvalidState)
    }
    fn get_bytes(&self, _key: &Key) -> airframe_data::error::Result<Option<Vec<u8>>> {
        Err(airframe_data::error::AirframeDataError::InvalidState)
    }
    fn remove(&self, _key: &Key) -> airframe_data::error::Result<()> {
        Err(airframe_data::error::AirframeDataError::InvalidState)
    }
    fn contains(&self, _key: &Key) -> airframe_data::error::Result<bool> {
        Err(airframe_data::error::AirframeDataError::InvalidState)
    }
    fn list(&self) -> airframe_data::error::Result<Vec<Key>> {
        Err(airframe_data::error::AirframeDataError::InvalidState)
    }
}

#[test]
fn cache_miss_then_hit_for_bytes() {
    let backend = MemBackend::new();
    let bytes = BackendByteCache::new(backend);
    let cache = SecretCache::new(bytes);

    let suite = SoftwareCipherSuite::new();
    let enc_key = SecretBytes::from_vec(vec![7u8; 32]);
    let k = Key::new("misc:bytes").unwrap();

    // Miss
    let got = cache
        .get_decrypted_bytes(&k, &suite, &enc_key, None)
        .unwrap();
    assert!(got.is_none());

    // Put and hit
    let pt = SecretBytes::from_vec(b"hello-secret".to_vec());
    cache
        .put_encrypted_bytes(&k, &suite, SymmetricAlgorithm::AesGcm, &enc_key, &pt, None)
        .unwrap();
    let got = cache
        .get_decrypted_bytes(&k, &suite, &enc_key, None)
        .unwrap();
    assert_eq!(got.unwrap(), b"hello-secret");
}

#[test]
fn cache_miss_then_hit_for_typed_value() {
    let backend = MemBackend::new();
    let bytes = BackendByteCache::new(backend);
    let cache = SecretCache::new(bytes);

    let suite = SoftwareCipherSuite::new();
    let enc_key = SecretBytes::from_vec(vec![8u8; 32]);
    let k = Key::new("misc:value").unwrap();

    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
    struct Demo {
        a: u32,
        b: String,
    }

    // Miss
    let got: Option<Demo> = cache.get_value(&k, &suite, &enc_key, None).unwrap();
    assert!(got.is_none());

    // Put and hit
    let v = Demo {
        a: 11,
        b: "eleven".into(),
    };
    cache
        .put_value(
            &k,
            &suite,
            SymmetricAlgorithm::ChaCha20Poly1305,
            &enc_key,
            &v,
            None,
        )
        .unwrap();
    let got: Option<Demo> = cache.get_value(&k, &suite, &enc_key, None).unwrap();
    assert_eq!(got.unwrap(), v);
}

#[test]
fn permission_error_mapping() {
    let cache = SecretCache::new(FailingCache);
    let suite = SoftwareCipherSuite::new();
    let enc_key = SecretBytes::from_vec(vec![1u8; 32]);
    let k = Key::new("fail:1").unwrap();

    // put should map inner error to AirframeSecretsError::InvalidState
    let res = cache.put_encrypted_bytes(
        &k,
        &suite,
        SymmetricAlgorithm::AesGcm,
        &enc_key,
        &SecretBytes::from_vec(vec![1]),
        None,
    );
    assert!(matches!(
        res,
        Err(airframe_secrets::error::AirframeSecretsError::InvalidState)
    ));

    // get should also map to InvalidState
    let res = cache.get_decrypted_bytes(&k, &suite, &enc_key, None);
    assert!(matches!(
        res,
        Err(airframe_secrets::error::AirframeSecretsError::InvalidState)
    ));
}
