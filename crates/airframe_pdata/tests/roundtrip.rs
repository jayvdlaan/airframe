use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_data::backend::mem::MemBackend;
use airframe_data::cache::BackendByteCache;
use airframe_data::codec::JsonCodec;
use airframe_data::key::Key;
use airframe_pdata::builder::PDataBuilder;
use airframe_pdata::bytes::PStoreBytes;
use airframe_pdata::context::{KeyResolver, PContext};
use airframe_pdata::typed::PStore;
use airframe_secrets::secret::SecretBytes;

struct StaticResolver {
    key: SecretBytes,
}
impl KeyResolver for StaticResolver {
    fn resolve(
        &self,
        _key_id: Option<&[u8]>,
    ) -> Result<SecretBytes, airframe_pdata::AirframePdataError> {
        // clone key by making a new SecretBytes from bytes
        let v = self.key.to_vec();
        Ok(SecretBytes::from_vec(v))
    }
}

#[test]
fn bytes_roundtrip() {
    let backend = MemBackend::new();
    let bytes = BackendByteCache::new(backend);

    let suite = SoftwareCipherSuite::new();
    let resolver = StaticResolver {
        key: SecretBytes::from_vec(vec![7u8; 32]),
    };
    let mut ctx = PContext::new(suite, SymmetricAlgorithm::ChaCha20Poly1305, resolver);
    ctx.namespace = Some("ns".into());

    let store: PStoreBytes<_, _> = PStoreBytes::new(bytes, ctx);
    let k = Key::new("demo").unwrap();
    let data = b"hello pdata".to_vec();
    store.put_bytes(&k, &data).unwrap();
    let out = store.get_bytes(&k).unwrap().unwrap();
    assert_eq!(out, data);
}

#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
struct Demo {
    a: u32,
    b: String,
}

#[test]
fn typed_roundtrip() {
    let backend = MemBackend::new();
    let bytes = BackendByteCache::new(backend);

    let suite = SoftwareCipherSuite::new();
    let resolver = StaticResolver {
        key: SecretBytes::from_vec(vec![5u8; 32]),
    };
    let mut ctx = PContext::new(suite, SymmetricAlgorithm::AesGcm, resolver);
    ctx.namespace = Some("data".into());

    let pbytes = PDataBuilder::new()
        .bytes(bytes)
        .context(ctx)
        .build_bytes()
        .unwrap();
    let pstore: PStore<_, _, _> = PStore::new(JsonCodec, pbytes);
    let k = Key::new("obj:1").unwrap();
    let v = Demo {
        a: 42,
        b: "life".into(),
    };
    pstore.put(&k, &v).unwrap();
    let out: Demo = pstore.get(&k).unwrap().unwrap();
    assert_eq!(out, v);
}
