use airframe_crypt::envelope::EnvelopeBytes;
use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_secrets::{SecretBlob, SecretBytes};

#[test]
fn secret_blob_with_plaintext_closure() {
    let suite = SoftwareCipherSuite::new();
    let key = SecretBytes::from_vec(vec![5u8; 32]);
    let aad = Some(b"ctx:aad".as_ref());

    let pt = SecretBytes::from_vec(Vec::from("blob secret"));
    let env = key
        .with_secrecy_slice(|k| {
            pt.with_secrecy_slice(|p| {
                EnvelopeBytes::encrypt_with_suite(&suite, SymmetricAlgorithm::AesGcm, k, p, aad)
            })
        })
        .unwrap();

    let blob = SecretBlob::new(env, None);
    let got = blob
        .with_plaintext(&suite, &key, aad, |p| p.to_vec())
        .unwrap();
    assert_eq!(got, b"blob secret");
}
