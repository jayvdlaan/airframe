#![cfg(feature = "protected")]

use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_data::codec::JsonCodec;
use airframe_data::key::Key;
use airframe_pdata::context::{KeyResolver, PContext};
use airframe_sdata::model::DataModel;
use airframe_sdata::protected::SDataProtectedFsBuilder;
use airframe_sdata::schema::SchemaRegistry;
use airframe_secrets::secret::SecretBytes;
use std::sync::Arc;
use tempfile::tempdir;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
struct Item {
    id: String,
    v: u32,
}
impl DataModel for Item {
    const SCHEMA_NAME: &'static str = "item";
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
fn fs_roundtrip_protected_repo() {
    let dir = tempdir().unwrap();
    let reg = Arc::new(SchemaRegistry::new());
    let suite = SoftwareCipherSuite::new();
    let resolver = StaticResolver(SecretBytes::from_vec(vec![9u8; 32]));
    let mut ctx = PContext::new(suite, SymmetricAlgorithm::AesGcm, resolver);
    ctx.namespace = Some("items".into());

    let repo = SDataProtectedFsBuilder::new(dir.path(), "dat")
        .context(ctx)
        .registry(reg)
        .build_typed::<JsonCodec, Item>(JsonCodec)
        .unwrap();

    let k = Key::new("one").unwrap();
    let val = Item {
        id: "one".into(),
        v: 1,
    };
    repo.put(&k, &val).unwrap();
    let out = repo.get(&k).unwrap().unwrap();
    assert_eq!(out, val);

    let keys = repo.list().unwrap();
    assert_eq!(keys.len(), 1);
}
