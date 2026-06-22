use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_data::backend::mem::MemBackend;
use airframe_data::cache::BackendByteCache;
use airframe_data::key::Key;
use airframe_secrets::{KeyResolver, SecretBytes, SecretCache};

// Simple resolver that always returns the same key
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend = MemBackend::new();
    let bytes = BackendByteCache::new(backend);
    let cache = SecretCache::new(bytes);

    let suite = SoftwareCipherSuite::new();
    let key = SecretBytes::from_vec(vec![3u8; 32]);
    let resolver = StaticResolver::new(key);

    // Typed value round-trip via resolver
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq, Debug)]
    struct Demo {
        a: u32,
        b: String,
    }

    let k1 = Key::new("resolver:typed")?;
    let v1 = Demo {
        a: 7,
        b: "seven".into(),
    };
    cache.put_value_resolved(
        &k1,
        &suite,
        SymmetricAlgorithm::AesGcm,
        &resolver,
        None,
        &v1,
        None,
    )?;
    let out1: Demo = cache
        .get_value_resolved(&k1, &suite, &resolver, None, None)?
        .expect("present");
    println!("resolver typed round-trip ok: {:?}", out1);

    // Raw bytes round-trip via resolver
    let k2 = Key::new("resolver:bytes")?;
    let pt = SecretBytes::from_vec(Vec::from("resolver secrets"));
    cache.put_encrypted_bytes_resolved(
        &k2,
        &suite,
        SymmetricAlgorithm::ChaCha20Poly1305,
        &resolver,
        None,
        &pt,
        None,
    )?;
    let out2 = cache
        .get_decrypted_bytes_resolved(&k2, &suite, &resolver, None, None)?
        .expect("present");
    println!(
        "resolver bytes round-trip ok: {}",
        String::from_utf8_lossy(&out2)
    );

    Ok(())
}
