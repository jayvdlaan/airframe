#![cfg(feature = "protected")]

use std::sync::Arc;

use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_data::backend::mem::MemBackend;
use airframe_data::cache::BackendByteCache;
use airframe_data::codec::JsonCodec;
use airframe_data::key::Key;
use airframe_pdata::context::{KeyResolver, PContext};
use airframe_pdata::policy::IndexedAadPolicy;
use airframe_sdata::error::AirframeSdataError;
use airframe_sdata::model::DataModel;
use airframe_sdata::protected::{ProtectedTypedRepo, SDataProtectedBuilder};
use airframe_sdata::schema::SchemaRegistry;
use airframe_secrets::secret::SecretBytes;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
struct Record {
    id: String,
    n: u32,
}
impl DataModel for Record {
    const SCHEMA_NAME: &'static str = "rec";
    const SCHEMA_VERSION: u32 = 1;
}

struct StaticResolver(SecretBytes);
impl KeyResolver for StaticResolver {
    fn resolve(
        &self,
        _key_id: Option<&[u8]>,
    ) -> Result<SecretBytes, airframe_pdata::AirframePdataError> {
        Ok(SecretBytes::from_vec(self.0.to_vec()))
    }
}

#[test]
fn aad_tamper_detection_with_index_end_to_end() {
    // Build mem backend protected repo
    let bytes = BackendByteCache::new(MemBackend::new());
    let reg = Arc::new(SchemaRegistry::new());

    let suite = SoftwareCipherSuite::new();
    let resolver = StaticResolver(SecretBytes::from_vec(vec![7u8; 32]));
    let mut ctx = PContext::new(suite, SymmetricAlgorithm::AesGcm, resolver);
    ctx.namespace = Some("recspace".into());
    ctx = ctx.with_aad_policy(Arc::new(IndexedAadPolicy::new(true)));

    let builder = SDataProtectedBuilder::new()
        .bytes(bytes)
        .context(ctx)
        .registry(reg);
    let repo: ProtectedTypedRepo<_, _, _, Record> = builder.build_typed(JsonCodec).unwrap();

    let k = Key::new("r:1").unwrap();
    let v = Record {
        id: "r:1".into(),
        n: 1,
    };

    // Store with one index bytes
    repo.put_with_index(&k, &v, b"ix-A").unwrap();

    // Attempt to get with different index bytes should fail authentication (InvalidState from sdata)
    let res = repo.get_with_index(&k, b"ix-B");
    assert!(matches!(res, Err(AirframeSdataError::InvalidState)));
}
