use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_data::backend::mem::MemBackend;
use airframe_data::cache::BackendByteCache;
use airframe_data::key::Key;
use airframe_pdata::bytes::PStoreBytes;
use airframe_pdata::context::{KeyResolver, PContext};
use airframe_secrets::secret::SecretBytes;

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = BackendByteCache::new(MemBackend::new());
    let resolver = StaticResolver {
        key: SecretBytes::from_vec(vec![9u8; 32]),
    };
    let ctx = PContext::new(
        SoftwareCipherSuite::new(),
        SymmetricAlgorithm::AesGcm,
        resolver,
    );

    let store = PStoreBytes::new(bytes, ctx);
    let k = Key::new("blob:demo")?;
    let data = b"protected hello".to_vec();
    store.put_bytes(&k, &data)?;
    let out = store.get_bytes(&k)?.unwrap();
    assert_eq!(out, data);
    println!("bytes_mem ok: {} bytes", out.len());
    Ok(())
}
