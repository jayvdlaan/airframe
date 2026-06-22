use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_data::backend::mem::MemBackend;
use airframe_data::cache::BackendByteCache;
use airframe_data::key::Key;
use airframe_pdata::builder::PDataBuilder;
use airframe_pdata::bytes::PStoreBytes;
use airframe_pdata::context::{KeyResolver, PContext};
use airframe_pdata::policy::IndexedAadPolicy;
use airframe_pdata::{AirframePdataError, Result};
use std::sync::Arc;

#[derive(Clone)]
struct StaticResolver(Vec<u8>);
impl KeyResolver for StaticResolver {
    fn resolve(&self, _key_id: Option<&[u8]>) -> Result<airframe_secrets::SecretBytes> {
        Ok(airframe_secrets::SecretBytes::from_vec(self.0.clone()))
    }
}

fn ctx_with_key(key: u8) -> PContext<StaticResolver> {
    let suite = SoftwareCipherSuite::new();
    let resolver = StaticResolver(vec![key; 32]);
    let mut ctx = PContext::new(suite, SymmetricAlgorithm::AesGcm, resolver);
    ctx.namespace = Some("ns".into());
    ctx = ctx.with_aad_policy(Arc::new(IndexedAadPolicy::new(true)));
    ctx
}

fn mem_bytes_with_bc(
    bc: BackendByteCache<MemBackend>,
    ctx: PContext<StaticResolver>,
) -> PStoreBytes<BackendByteCache<MemBackend>, StaticResolver> {
    PDataBuilder::new()
        .bytes(bc)
        .context(ctx)
        .build_bytes()
        .unwrap()
}

#[test]
fn aad_tamper_with_index_meta_fails() {
    let ctx = ctx_with_key(7);
    let bc = BackendByteCache::new(MemBackend::new());
    let store = mem_bytes_with_bc(bc, ctx.clone());
    let k = Key::new("t:1").unwrap();

    store
        .put_bytes_with_meta(&k, b"hello", Some(b"indexA"))
        .unwrap();
    // Different index bytes should cause auth failure
    let res = store.get_bytes_with_meta(&k, Some(b"indexB"));
    assert!(matches!(res, Err(AirframePdataError::InvalidState)));
}

#[test]
fn wrong_factor_cannot_decrypt() {
    let ctx_a = ctx_with_key(1);
    let bc = BackendByteCache::new(MemBackend::new());
    let store_a = mem_bytes_with_bc(bc.clone(), ctx_a.clone());
    let k = Key::new("t:2").unwrap();
    store_a.put_bytes(&k, b"secret").unwrap();

    // New store with different key but same backend
    let ctx_b = ctx_with_key(2);
    let store_b = mem_bytes_with_bc(bc, ctx_b.clone());
    let res = store_b.get_bytes(&k);
    assert!(matches!(res, Err(AirframePdataError::InvalidState)));
}

#[test]
fn rewrap_moves_to_new_context() {
    let ctx1 = ctx_with_key(3);
    let bc = BackendByteCache::new(MemBackend::new());
    let store1 = mem_bytes_with_bc(bc.clone(), ctx1.clone());
    let k = Key::new("t:3").unwrap();
    store1.put_bytes(&k, b"payload").unwrap();

    let ctx2 = ctx_with_key(4);
    // Rewrap using store1 API to ctx2
    let ok = store1.rewrap_to(&k, &ctx2).unwrap();
    assert!(ok);

    // Old context should fail
    assert!(store1.get_bytes(&k).is_err());

    // New store with ctx2 over same backend should succeed
    let store2 = mem_bytes_with_bc(bc, ctx2.clone());
    let got = store2.get_bytes(&k).unwrap().unwrap();
    assert_eq!(got, b"payload");
}
