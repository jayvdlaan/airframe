// Build with:
// cargo run -p airframe_sdata --example protected_schema_cache_mem --features airframe_sdata/integration-pdata
// (legacy alias also works: --features airframe_sdata/protected)

#[cfg(any(feature = "integration-pdata", feature = "protected"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use airframe_crypt::suite::SoftwareCipherSuite;
    use airframe_crypt::sym::SymmetricAlgorithm;
    use airframe_data::backend::mem::MemBackend;
    use airframe_data::cache::BackendByteCache;
    use airframe_data::codec::JsonCodec;
    use airframe_data::key::Key;
    use airframe_pdata::context::{KeyResolver, PContext};
    use airframe_sdata::cache::{ProtectedSchemaCache, SDataProtectedCacheBuilder};
    use airframe_sdata::model::DataModel;
    use airframe_sdata::schema::{Migrator, SchemaRegistry};
    use airframe_secrets::secret::SecretBytes;
    use std::sync::Arc;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
    struct UserV2 {
        id: String,
        name: String,
        age: u32,
    }
    impl DataModel for UserV2 {
        const SCHEMA_NAME: &'static str = "user";
        const SCHEMA_VERSION: u32 = 2;
    }

    struct UserMigV1toV2;
    impl Migrator for UserMigV1toV2 {
        fn schema_name(&self) -> &'static str {
            "user"
        }
        fn migrate(
            &self,
            _from: u32,
            _to: u32,
            mut v: serde_json::Value,
        ) -> airframe_sdata::error::Result<serde_json::Value> {
            if let Some(obj) = v.as_object_mut() {
                obj.entry("age").or_insert(serde_json::json!(0));
            }
            Ok(v)
        }
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

    // Bytes backend via ByteCache
    let bytes = BackendByteCache::new(MemBackend::new());

    // Schema registry
    let mut reg = SchemaRegistry::new();
    reg.register_step("user", 1, Arc::new(UserMigV1toV2));
    let reg = Arc::new(reg);

    // Encryption context (pdata)
    let resolver = StaticResolver {
        key: SecretBytes::from_vec(vec![7u8; 32]),
    };
    let suite = SoftwareCipherSuite::new();
    let mut ctx = PContext::new(suite, SymmetricAlgorithm::AesGcm, resolver);
    ctx.namespace = Some("users".into());

    // Build protected schema cache
    let cache: ProtectedSchemaCache<_, _, _, UserV2> = SDataProtectedCacheBuilder::new()
        .bytes(bytes)
        .context(ctx)
        .registry(reg)
        .build_typed(JsonCodec)?;

    // Put/Get
    let k = Key::new("user:carol")?;
    let v2 = UserV2 {
        id: "carol".into(),
        name: "Carol".into(),
        age: 0,
    };
    cache.put(&k, &v2)?;
    let out = cache.get(&k)?.unwrap();
    assert_eq!(out, v2);

    println!("protected_schema_cache_mem ok: {:?}", out);
    Ok(())
}

#[cfg(not(any(feature = "integration-pdata", feature = "protected")))]
fn main() {
    println!("Enable feature 'integration-pdata' to run this example: --features airframe_sdata/integration-pdata\n(legacy alias also works: --features airframe_sdata/protected)");
}
