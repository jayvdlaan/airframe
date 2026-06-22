use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_data::backend::mem::MemBackend;
use airframe_data::cache::BackendByteCache;
use airframe_data::key::Key;
use airframe_secrets::{KeyResolver, SecretBytes, SecretCache};

// A trivial static resolver that always returns the same key regardless of key_id
struct StaticResolver(SecretBytes);
impl KeyResolver for StaticResolver {
    fn resolve(
        &self,
        _key_id: Option<&[u8]>,
    ) -> Result<SecretBytes, airframe_secrets::error::AirframeSecretsError> {
        Ok(self.0.expose(|k| SecretBytes::from_vec(k.to_vec())))
    }
}

// cargo run -q -p airframe_secrets --example secret_fallback
fn main() -> anyhow::Result<()> {
    let suite = SoftwareCipherSuite::new();
    let backend = MemBackend::new();
    let bytes = BackendByteCache::new(backend);
    let cache = SecretCache::new(bytes);

    // App-specific secret identifier and default value
    let key_id: Option<&[u8]> = None;
    let cache_key = Key::new("service:api_token")?;
    let default_secret = b"default-token".to_vec();

    // In a real app, your resolver would fetch a KMS/HSM-derived key, or load from OS store
    let resolver = StaticResolver(SecretBytes::from_vec(vec![0xAB; 32]));

    // Try cache; if miss, fallback to default and store encrypted
    let secret: Vec<u8> =
        match cache.get_decrypted_bytes_resolved(&cache_key, &suite, &resolver, key_id, None)? {
            Some(pt) => pt,
            None => {
                // Use default and persist encrypted for next time
                let pt = SecretBytes::from_vec(default_secret.clone());
                cache.put_encrypted_bytes_resolved(
                    &cache_key,
                    &suite,
                    SymmetricAlgorithm::AesGcm,
                    &resolver,
                    key_id,
                    &pt,
                    None,
                )?;
                default_secret
            }
        };

    println!(
        "Loaded secret ({} bytes); cached? {}",
        secret.len(),
        cache.contains(&cache_key).unwrap_or(false)
    );
    Ok(())
}
