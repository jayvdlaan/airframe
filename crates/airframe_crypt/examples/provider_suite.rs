use airframe_crypt::asym;
use airframe_crypt::hash::DigestAlgorithm;
use airframe_crypt::suite::{
    OpenSslAsymProvider, OpenSslHashProvider, OpenSslKdfProvider, OpenSslKeyWrapProvider,
    OpenSslRandomProvider, OpenSslSymProvider, PrivateKey, ProviderCipherSuite, PublicKey,
    TotpRsProvider,
};
use airframe_crypt::sym::SymmetricAlgorithm;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn main() {
    // Compose a provider-backed suite
    let suite = ProviderCipherSuite::new()
        .with_hash_provider(OpenSslHashProvider)
        .with_random_provider(OpenSslRandomProvider)
        .with_sym_provider(OpenSslSymProvider)
        .with_kdf_provider(OpenSslKdfProvider)
        .with_keywrap_provider(OpenSslKeyWrapProvider)
        .with_asym_provider(OpenSslAsymProvider)
        .with_otp_provider(TotpRsProvider);

    // Hash and HMAC
    let d = suite
        .digest(
            DigestAlgorithm::Sha256,
            b"The quick brown fox jumps over the lazy dog",
        )
        .unwrap();
    println!("sha256 = {}", hex(&d));

    let key = vec![0x0b; 20];
    let mac = suite
        .hmac(DigestAlgorithm::Sha256, &key, b"Hi There")
        .unwrap();
    println!("hmac-sha256 = {}", hex(&mac));

    // Random bytes
    let r = suite.random_bytes(16).unwrap();
    println!("random[16] = {} bytes", r.len());

    // Symmetric encrypt/decrypt (AES-GCM)
    let aes_key = vec![0x11; 16];
    let nonce = vec![0x22; 12];
    let msg = b"Hello Suite";
    let ct = suite
        .sym_encrypt(SymmetricAlgorithm::AesGcm, &aes_key, &nonce, msg, None)
        .unwrap();
    let pt = suite
        .sym_decrypt(SymmetricAlgorithm::AesGcm, &aes_key, &nonce, &ct, None)
        .unwrap();
    assert_eq!(pt, msg);
    println!("AES-GCM roundtrip OK, ct {} bytes", ct.len());

    // KDF PBKDF2
    let derived = suite
        .pbkdf2(
            b"password",
            b"salt",
            1000,
            airframe_crypt::kdf::Pbkdf2Digest::Sha256,
            32,
        )
        .unwrap();
    println!("pbkdf2 len = {}", derived.len());

    // Key wrap/unwrap (RFC3394)
    let kek = vec![0xAA; 16];
    let plaintext_key = vec![0x55; 16];
    let wrapped = suite.wrap_key(&kek, &plaintext_key).unwrap();
    let unwrapped = suite.unwrap_key(&kek, &wrapped).unwrap();
    assert_eq!(unwrapped, plaintext_key);
    println!("key wrap roundtrip OK");

    // Asymmetric: RSA sign/verify. Keys cross the suite boundary as backend-agnostic
    // PEM wrappers — no OpenSSL types appear in the API.
    let rsa_priv = asym::openssl_rsa_generate(2048).unwrap();
    let priv_key = PrivateKey::from_pem(rsa_priv.private_key_to_pem_pkcs8().unwrap());
    let pub_key = PublicKey::from_pem(rsa_priv.public_key_to_pem().unwrap());
    let sig = suite
        .asym_sign(asym::AsymSignAlgorithm::RsaPssSha256, &priv_key, b"sign me")
        .unwrap();
    assert!(suite
        .asym_verify(
            asym::AsymSignAlgorithm::RsaPssSha256,
            &pub_key,
            b"sign me",
            &sig
        )
        .unwrap());
    println!("RSA-PSS sign/verify OK");

    // OTP TOTP roundtrip
    let secret = b"0123456789abcdef"; // 16-byte secret
    let code = suite
        .totp_generate_current(airframe_crypt::otp::OtpAlgorithm::Sha1, secret, 6, 30, 1)
        .unwrap();
    let ok = suite
        .totp_verify_current(
            airframe_crypt::otp::OtpAlgorithm::Sha1,
            secret,
            &code,
            6,
            30,
            1,
        )
        .unwrap();
    println!("TOTP generated {} verified? {}", code, ok);
}
