use airframe_crypt::hash::DigestAlgorithm;
use airframe_crypt::suite::{CipherSuite, SoftwareCipherSuite};
use airframe_crypt::sym::SymmetricAlgorithm;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn main() {
    let suite = SoftwareCipherSuite::new();
    let d = suite.digest(DigestAlgorithm::Sha256, b"abc").unwrap();
    println!("sha256(abc) = {}", hex(&d));

    let key = vec![0x11; 16];
    let nonce = vec![0x22; 12];
    let msg = b"hello crypt";
    let ct = suite
        .sym_encrypt(SymmetricAlgorithm::AesGcm, &key, &nonce, msg, None)
        .unwrap();
    let pt = suite
        .sym_decrypt(SymmetricAlgorithm::AesGcm, &key, &nonce, &ct, None)
        .unwrap();
    assert_eq!(pt, msg);
    println!("AES-GCM roundtrip OK, ct {} bytes", ct.len());
}
