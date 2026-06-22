// Derive a key with PBKDF2-HMAC-SHA256 and encrypt/decrypt a message with AES-256-GCM.
// Run: cargo run -q -p airframe_crypt --example derive_encrypt

use airframe_crypt::kdf::{openssl_derive_pbkdf2, Pbkdf2Digest};
use airframe_crypt::sym::{openssl_sym_decrypt, openssl_sym_encrypt, SymmetricAlgorithm};
use zeroize::Zeroizing;

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

fn main() -> anyhow::Result<()> {
    // Derive a 32-byte key from password and salt.
    let password = b"correct horse battery staple";
    let salt = b"example-salt"; // In production, use a per-user random salt (≥16 bytes recommended)
    let iterations = 100_000; // Tune based on performance/security requirements

    // Use Zeroizing to ensure the derived key is wiped when it goes out of scope
    let mut key = Zeroizing::new(vec![0u8; 32]);
    openssl_derive_pbkdf2(password, salt, iterations, Pbkdf2Digest::Sha256, &mut key)?;

    // A 12-byte random nonce is typical for GCM. For a demo, use a fixed value (do NOT do this in production).
    // Always use a unique nonce per key to maintain AEAD security.
    let nonce = b"unique-nonce12"; // 12 bytes

    // Encrypt a message. Optionally include AAD that must match for decryption.
    let aad = b"demo-aad";
    let message = b"Encryption works!";
    let ciphertext =
        openssl_sym_encrypt(SymmetricAlgorithm::AesGcm, &key, nonce, message, Some(aad))?;

    println!("derived key (hex): {}", hex(&key));
    println!("ciphertext+tag (hex): {}", hex(&ciphertext));

    // Decrypt and verify
    let plaintext = openssl_sym_decrypt(
        SymmetricAlgorithm::AesGcm,
        &key,
        nonce,
        &ciphertext,
        Some(aad),
    )?;
    println!("plaintext: {}", String::from_utf8_lossy(&plaintext));

    Ok(())
}
