use std::sync::Arc;

use airframe_core::app::AppBuilder;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_data::key::Key;
use airframe_pdata::module::{PDataModule, ServiceRegistryPDataExt};

struct StaticResolver;
impl airframe_secrets::KeyResolver for StaticResolver {
    fn resolve(
        &self,
        _key_id: Option<&[u8]>,
    ) -> airframe_secrets::error::Result<airframe_secrets::SecretBytes> {
        Ok(airframe_secrets::SecretBytes::from_vec(vec![9u8; 32]))
    }
}

#[tokio::main]
async fn main() {
    let app = AppBuilder::new()
        .with(airframe_crypt::CryptModule::new())
        // Optional, but shows capability graph correctness
        .with(airframe_secrets::SecretsModule::new())
        .with(PDataModule::new())
        .start()
        .await
        .unwrap();

    let pd = app.services.pdata_factory().expect("PDataFactory present");
    let ctx = pd.context_with_secrets(
        SymmetricAlgorithm::ChaCha20Poly1305,
        Arc::new(StaticResolver),
    );

    let bytes = pd.bytes_mem(ctx.clone());
    let k = Key::new("ex:1").unwrap();
    bytes.put_bytes(&k, b"hello").unwrap();
    let out = bytes.get_bytes(&k).unwrap().unwrap();
    println!("roundtrip bytes: {}", String::from_utf8_lossy(&out));

    #[derive(serde::Serialize, serde::Deserialize, Debug)]
    struct Demo {
        a: u32,
    }
    let typed = pd.typed_json_mem(ctx.clone());
    let k2 = Key::new("ex:2").unwrap();
    let v = Demo { a: 7 };
    typed.put(&k2, &v).unwrap();
    let out: Demo = typed.get(&k2).unwrap().unwrap();
    println!("roundtrip typed: {:?}", out);
}
