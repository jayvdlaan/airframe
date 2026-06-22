use airframe_crypt::envelope::EnvelopeBytes;
use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_secrets::{SecretBlob, SecretBytes};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let suite = SoftwareCipherSuite::new();
    let key = SecretBytes::from_vec(vec![5u8; 32]);
    let aad = Some(b"example:aad".as_ref());

    // Encrypt some plaintext bytes into an envelope
    let pt = SecretBytes::from_vec(Vec::from("blob secret"));
    let env = key.with_secrecy_slice(|k| {
        pt.with_secrecy_slice(|p| {
            EnvelopeBytes::encrypt_with_suite(&suite, SymmetricAlgorithm::AesGcm, k, p, aad)
        })
    })?;

    // Wrap as SecretBlob and access plaintext only within a closure
    let blob = SecretBlob::new(env, None);
    let copied: Vec<u8> = blob.with_plaintext(&suite, &key, aad, |p| p.to_vec())?;

    println!(
        "secret blob decrypted: {}",
        String::from_utf8_lossy(&copied)
    );
    Ok(())
}
