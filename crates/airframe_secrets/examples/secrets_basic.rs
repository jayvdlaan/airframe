use airframe_core::app::AppBuilder;
use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_data::key::Key;
use airframe_secrets::{SecretBytes, SecretsModule, ServiceRegistrySecretsExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new()
        .with(airframe_crypt::CryptModule::new())
        .with(SecretsModule::new())
        .start()
        .await?;

    let cache = app.services.secrets_cache().expect("SecretCache present");
    let suite: std::sync::Arc<SoftwareCipherSuite> =
        app.services.get::<SoftwareCipherSuite>().expect("suite");

    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
    struct Demo {
        a: u32,
        b: String,
    }

    let key = SecretBytes::from_vec(vec![7u8; 32]);
    let entry = Key::new("demo:1")?;
    let val = Demo {
        a: 42,
        b: "life".into(),
    };

    cache.put_value(
        &entry,
        &*suite,
        SymmetricAlgorithm::AesGcm,
        &key,
        &val,
        None,
    )?;
    let out: Demo = cache.get_value(&entry, &*suite, &key, None)?.unwrap();
    println!("roundtrip = {:?}", out);

    app.cancel.cancel();
    Ok(())
}
