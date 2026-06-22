use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_data::backend::mem::MemBackend;
use airframe_data::cache::BackendByteCache;
use airframe_data::key::Key;
use airframe_secrets::{SecretBytes, SecretCache};

#[test]
fn secret_cache_roundtrip_value() {
    let backend = MemBackend::new();
    let bytes = BackendByteCache::new(backend);
    let cache = SecretCache::new(bytes);

    let suite = SoftwareCipherSuite::new();
    let key = SecretBytes::from_vec(vec![7u8; 32]);

    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq, Debug)]
    struct Demo {
        a: u32,
        b: String,
    }

    let k = Key::new("demo:1").unwrap();
    let v = Demo {
        a: 42,
        b: "life".into(),
    };

    cache
        .put_value(&k, &suite, SymmetricAlgorithm::AesGcm, &key, &v, None)
        .unwrap();
    let out: Demo = cache.get_value(&k, &suite, &key, None).unwrap().unwrap();
    assert_eq!(out, v);
}

#[test]
fn secret_cache_roundtrip_bytes() {
    let backend = MemBackend::new();
    let bytes = BackendByteCache::new(backend);
    let cache = SecretCache::new(bytes);

    let suite = SoftwareCipherSuite::new();
    let key = SecretBytes::from_vec(vec![9u8; 32]);

    let k = Key::new("raw:bytes").unwrap();
    let pt = SecretBytes::from_vec(Vec::from("hello secrets"));

    cache
        .put_encrypted_bytes(
            &k,
            &suite,
            SymmetricAlgorithm::ChaCha20Poly1305,
            &key,
            &pt,
            None,
        )
        .unwrap();
    let out = cache
        .get_decrypted_bytes(&k, &suite, &key, None)
        .unwrap()
        .unwrap();
    assert_eq!(out, b"hello secrets");
}
