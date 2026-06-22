#[cfg(any(feature = "integration-pdata", feature = "protected"))]
mod demo {
    use airframe_data::codec::JsonCodec;
    use airframe_data::{backend::mem::MemBackend, cache::BackendByteCache, key::Key};
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;

    use airframe_sdata::protected::SDataProtectedBuilder;
    use airframe_sdata::{model::DataModel, schema::SchemaRegistry};

    use airframe_crypt::{suite::SoftwareCipherSuite, sym::SymmetricAlgorithm};
    use airframe_pdata::context::{KeyResolver, PContext};
    use airframe_secrets::secret::SecretBytes;

    #[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
    struct UserV1 {
        id: String,
        name: String,
    }
    impl DataModel for UserV1 {
        const SCHEMA_NAME: &'static str = "user";
        const SCHEMA_VERSION: u32 = 1;
    }

    struct StaticResolver(SecretBytes);
    impl KeyResolver for StaticResolver {
        fn resolve(&self, _key_id: Option<&[u8]>) -> airframe_pdata::Result<SecretBytes> {
            Ok(SecretBytes::from_vec(self.0.to_vec()))
        }
    }

    pub fn run() -> anyhow::Result<()> {
        // Build pdata context
        let suite = SoftwareCipherSuite::new();
        let ctx = PContext::new(
            suite,
            SymmetricAlgorithm::ChaCha20Poly1305,
            StaticResolver(SecretBytes::from_vec(vec![7u8; 32])),
        );

        let reg = Arc::new(SchemaRegistry::new());
        let bc = BackendByteCache::new(MemBackend::new());

        let repo = SDataProtectedBuilder::new()
            .bytes(bc)
            .context(ctx)
            .registry(reg)
            .build_typed::<_, UserV1>(JsonCodec)?;

        let key = Key::new("users:42")?;
        let v = UserV1 {
            id: "42".into(),
            name: "Zoe".into(),
        };
        repo.put(&key, &v)?;
        let out = repo.get(&key)?.unwrap();
        assert_eq!(out, v);
        println!("protected roundtrip ok: {:?}", out);
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    #[cfg(any(feature = "integration-pdata", feature = "protected"))]
    {
        return demo::run();
    }
    #[allow(clippy::let_unit_value)]
    {
        println!("This example requires the 'integration-pdata' feature. Run with:\n  cargo run -p airframe_sdata --example protected_mem --features airframe_sdata/integration-pdata\n(legacy alias also works: --features airframe_sdata/protected)");
    }
    Ok(())
}
