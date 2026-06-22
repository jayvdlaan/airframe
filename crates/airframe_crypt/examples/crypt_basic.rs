use airframe_core::app::AppBuilder;
use airframe_crypt::{CryptModule, ServiceRegistryCryptExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new().with(CryptModule::new()).start().await?;

    let suite = app.services.crypt().expect("crypt suite present");

    // Digest
    let sha = suite.digest(
        airframe_crypt::hash::DigestAlgorithm::Sha256,
        b"hello world",
    )?;
    println!("sha256 len = {}", sha.len());

    // AEAD encrypt/decrypt with a random key
    use airframe_crypt::sym::SymmetricAlgorithm;
    let key = suite.random_bytes(32)?; // AES-256-GCM key
    let nonce = suite.random_bytes(12)?;
    let ct = suite.sym_encrypt(SymmetricAlgorithm::AesGcm, &key, &nonce, b"secret", None)?;
    let pt = suite.sym_decrypt(SymmetricAlgorithm::AesGcm, &key, &nonce, &ct, None)?;
    println!("decrypted: {}", String::from_utf8_lossy(&pt));

    app.cancel.cancel();
    Ok(())
}
