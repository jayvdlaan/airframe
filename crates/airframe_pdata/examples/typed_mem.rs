use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_data::backend::mem::MemBackend;
use airframe_data::cache::BackendByteCache;
use airframe_data::codec::JsonCodec;
use airframe_data::key::Key;
use airframe_pdata::builder::PDataBuilder;
use airframe_pdata::context::{KeyResolver, PContext};
use airframe_pdata::typed::PStore;
use airframe_secrets::secret::SecretBytes;

#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
struct User {
    id: u32,
    name: String,
}

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
        key: SecretBytes::from_vec(vec![7u8; 32]),
    };
    let suite = SoftwareCipherSuite::new();
    let mut ctx = PContext::new(suite, SymmetricAlgorithm::ChaCha20Poly1305, resolver);
    ctx.namespace = Some("users".into());

    let store: PStore<_, _, _> = PDataBuilder::new()
        .bytes(bytes)
        .context(ctx)
        .build_typed(JsonCodec)?;

    let k = Key::new("u:1")?;
    let v = User {
        id: 1,
        name: "Luna".into(),
    };
    store.put(&k, &v)?;
    let got: User = store.get(&k)?.unwrap();
    assert_eq!(got, v);
    println!("typed_mem ok: {:?}", got);
    Ok(())
}
