use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_data::backend::mem::MemBackend;
use airframe_data::cache::BackendByteCache;
use airframe_data::key::Key;
use airframe_secrets::{SecretBytes, SecretCache};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Backing cache: in-memory KV adapted to a ByteCache
    let backend = MemBackend::new();
    let bytes = BackendByteCache::new(backend);
    let cache = SecretCache::new(bytes);

    // Crypto primitives
    let suite = SoftwareCipherSuite::new();
    let key = SecretBytes::from_vec(vec![7u8; 32]);

    // Example 1: store a typed value encrypted (bincode inside an envelope)
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq, Debug)]
    struct Demo {
        a: u32,
        b: String,
    }

    let k1 = Key::new("demo:typed")?;
    let v1 = Demo {
        a: 42,
        b: "life".into(),
    };
    cache.put_value(&k1, &suite, SymmetricAlgorithm::AesGcm, &key, &v1, None)?;
    let out1: Demo = cache.get_value(&k1, &suite, &key, None)?.expect("present");
    println!("typed round-trip ok: {:?}", out1);

    // Example 2: store raw bytes encrypted
    let k2 = Key::new("demo:bytes")?;
    let pt = SecretBytes::from_vec(Vec::from("hello secrets"));
    cache.put_encrypted_bytes(
        &k2,
        &suite,
        SymmetricAlgorithm::ChaCha20Poly1305,
        &key,
        &pt,
        None,
    )?;
    let out2 = cache
        .get_decrypted_bytes(&k2, &suite, &key, None)?
        .expect("present");
    println!("bytes round-trip ok: {}", String::from_utf8_lossy(&out2));

    Ok(())
}
