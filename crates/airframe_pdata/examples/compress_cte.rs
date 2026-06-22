// Build with: cargo run -p airframe_pdata --example compress_cte --features airframe_pdata/compress

#[cfg(feature = "compress")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use airframe_crypt::suite::SoftwareCipherSuite;
    use airframe_crypt::sym::SymmetricAlgorithm;
    use airframe_data::backend::mem::MemBackend;
    use airframe_data::cache::BackendByteCache;
    use airframe_data::key::Key;
    use airframe_pdata::bytes::PStoreBytes;
    use airframe_pdata::context::{KeyResolver, PContext};
    use airframe_pdata::policy::Compression;
    use airframe_secrets::secret::SecretBytes;
    use std::sync::Arc;

    struct StaticResolver {
        key: SecretBytes,
    }
    impl KeyResolver for StaticResolver {
        fn resolve(
            &self,
            _key_id: Option<&[u8]>,
        ) -> Result<SecretBytes, airframe_pdata::AirframePdataError> {
            Ok(SecretBytes::from_vec(self.key.to_vec()))
        }
    }

    let bytes = BackendByteCache::new(MemBackend::new());
    let resolver = StaticResolver {
        key: SecretBytes::from_vec(vec![3u8; 32]),
    };
    let suite = SoftwareCipherSuite::new();
    let mut ctx = PContext::new(suite, SymmetricAlgorithm::AesGcm, resolver);
    ctx.namespace = Some("ns".into());
    ctx.compression = Compression::Algo(Arc::new(airframe_compress::Zstd::new(3)));

    let store = PStoreBytes::new(bytes, ctx);
    let k = Key::new("blob:zstd")?;
    let data: Vec<u8> = (0..50_000).map(|i| (i % 251) as u8).collect();
    store.put_bytes(&k, &data)?;
    let out = store.get_bytes(&k)?.unwrap();
    assert_eq!(out, data);
    println!("compress_cte ok: {} bytes", out.len());
    Ok(())
}

#[cfg(not(feature = "compress"))]
fn main() {
    println!("Enable feature 'compress' to run this example: --features airframe_pdata/compress");
}
