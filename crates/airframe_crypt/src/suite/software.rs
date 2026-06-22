use crate::asym;
use crate::error::AirframeCryptError;
use crate::hash::{self, DigestAlgorithm};
use crate::kdf::{self, Argon2Params, Pbkdf2Digest};
use crate::otp::{self, OtpAlgorithm};
use crate::sym::KeyWrapper;
use crate::sym::{self, OpenSSLKeyWrapper, SymmetricAlgorithm};
use openssl::pkey::{PKey, Private, Public};

use super::{
    AsymProvider, CipherSuite, HashProvider, KdfProvider, KeyWrapProvider, OtpProvider, PrivateKey,
    PublicKey, RandomProvider, SymProvider,
};

/// Software-only cipher suite backed by OpenSSL (hash/sym/asym/kdf/rand/keywrap) and totp-rs for OTP.
#[derive(Default, Debug, Clone, Copy)]
pub struct SoftwareCipherSuite;

impl SoftwareCipherSuite {
    pub fn new() -> Self {
        Self
    }
}

impl CipherSuite for SoftwareCipherSuite {
    fn digest(&self, alg: DigestAlgorithm, data: &[u8]) -> Result<Vec<u8>, AirframeCryptError> {
        hash::openssl_digest(alg, data)
    }
    fn hmac(
        &self,
        alg: DigestAlgorithm,
        key: &[u8],
        data: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError> {
        hash::openssl_hmac(alg, key, data)
    }

    fn random_bytes(&self, len: usize) -> Result<Vec<u8>, AirframeCryptError> {
        crate::rand::openssl_random_bytes(len).map_err(AirframeCryptError::from)
    }

    fn sym_encrypt(
        &self,
        alg: SymmetricAlgorithm,
        key: &[u8],
        iv_or_nonce: &[u8],
        plaintext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        sym::openssl_sym_encrypt(alg, key, iv_or_nonce, plaintext, aad)
    }
    fn sym_decrypt(
        &self,
        alg: SymmetricAlgorithm,
        key: &[u8],
        iv_or_nonce: &[u8],
        ciphertext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        sym::openssl_sym_decrypt(alg, key, iv_or_nonce, ciphertext, aad)
    }

    fn pbkdf2(
        &self,
        password: &[u8],
        salt: &[u8],
        iterations: usize,
        digest: Pbkdf2Digest,
        out_len: usize,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        let mut out = vec![0u8; out_len];
        kdf::openssl_derive_pbkdf2(password, salt, iterations, digest, &mut out)?;
        Ok(out)
    }

    fn argon2(
        &self,
        password: &[u8],
        salt: &[u8],
        params: Argon2Params,
        out_len: usize,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        kdf::rustcrypto_derive_argon2(password, salt, params, out_len)
    }

    fn wrap_key(&self, kek: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, AirframeCryptError> {
        let wrapper = OpenSSLKeyWrapper::new(kek.to_vec())?;
        wrapper.wrap_key(plaintext)
    }
    fn unwrap_key(&self, kek: &[u8], wrapped: &[u8]) -> Result<Vec<u8>, AirframeCryptError> {
        let wrapper = OpenSSLKeyWrapper::new(kek.to_vec())?;
        wrapper.unwrap_key(wrapped)
    }

    fn asym_encrypt(
        &self,
        alg: asym::AsymEncryptAlgorithm,
        public_key: &PublicKey,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError> {
        let pk: PKey<Public> = PKey::public_key_from_pem(public_key.as_pem())?;
        asym::openssl_asym_encrypt(alg, &pk, plaintext)
    }
    fn asym_decrypt(
        &self,
        alg: asym::AsymEncryptAlgorithm,
        private_key: &PrivateKey,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError> {
        let sk: PKey<Private> = PKey::private_key_from_pem(private_key.as_pem())?;
        asym::openssl_asym_decrypt(alg, &sk, ciphertext)
    }
    fn asym_sign(
        &self,
        alg: asym::AsymSignAlgorithm,
        private_key: &PrivateKey,
        msg: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError> {
        let sk: PKey<Private> = PKey::private_key_from_pem(private_key.as_pem())?;
        asym::openssl_asym_sign(alg, &sk, msg)
    }
    fn asym_verify(
        &self,
        alg: asym::AsymSignAlgorithm,
        public_key: &PublicKey,
        msg: &[u8],
        sig: &[u8],
    ) -> Result<bool, AirframeCryptError> {
        let pk: PKey<Public> = PKey::public_key_from_pem(public_key.as_pem())?;
        asym::openssl_asym_verify(alg, &pk, msg, sig)
    }
    fn asym_derive(
        &self,
        alg: asym::AsymKexAlgorithm,
        my_private: &PrivateKey,
        peer_public: &PublicKey,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        let sk: PKey<Private> = PKey::private_key_from_pem(my_private.as_pem())?;
        let pk: PKey<Public> = PKey::public_key_from_pem(peer_public.as_pem())?;
        asym::openssl_asym_derive(alg, &sk, &pk)
    }

    fn totp_generate_current(
        &self,
        alg: OtpAlgorithm,
        secret: &[u8],
        digits: u32,
        step: u64,
        skew: u8,
    ) -> Result<String, AirframeCryptError> {
        otp::totprs_totp_generate_current(alg, secret, digits, step, skew)
    }
    fn totp_verify_current(
        &self,
        alg: OtpAlgorithm,
        secret: &[u8],
        code: &str,
        digits: u32,
        step: u64,
        skew: u8,
    ) -> Result<bool, AirframeCryptError> {
        otp::totprs_totp_verify_current(alg, secret, code, digits, step, skew)
    }
}

/// OpenSSL-backed hashing provider that delegates to hash.rs
pub struct OpenSslHashProvider;
impl HashProvider for OpenSslHashProvider {
    fn digest(&self, alg: DigestAlgorithm, data: &[u8]) -> Result<Vec<u8>, AirframeCryptError> {
        hash::openssl_digest(alg, data)
    }
    fn hmac(
        &self,
        alg: DigestAlgorithm,
        key: &[u8],
        data: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError> {
        hash::openssl_hmac(alg, key, data)
    }
}

/// OpenSSL-backed random provider that delegates to rand.rs one-shot
pub struct OpenSslRandomProvider;
impl RandomProvider for OpenSslRandomProvider {
    fn random_bytes(&self, len: usize) -> Result<Vec<u8>, AirframeCryptError> {
        // crate::rand::random_bytes returns Result<Vec<u8>, openssl::error::ErrorStack>
        // Convert to AirframeCryptError via the #[from] OpenSSLError impl
        crate::rand::openssl_random_bytes(len).map_err(AirframeCryptError::from)
    }
}

/// OpenSSL-backed symmetric provider
pub struct OpenSslSymProvider;
impl SymProvider for OpenSslSymProvider {
    fn encrypt(
        &self,
        alg: SymmetricAlgorithm,
        key: &[u8],
        iv_or_nonce: &[u8],
        plaintext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        sym::openssl_sym_encrypt(alg, key, iv_or_nonce, plaintext, aad)
    }

    fn decrypt(
        &self,
        alg: SymmetricAlgorithm,
        key: &[u8],
        iv_or_nonce: &[u8],
        ciphertext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        sym::openssl_sym_decrypt(alg, key, iv_or_nonce, ciphertext, aad)
    }
}

pub struct OpenSslKdfProvider;
impl KdfProvider for OpenSslKdfProvider {
    fn pbkdf2(
        &self,
        password: &[u8],
        salt: &[u8],
        iterations: usize,
        digest: Pbkdf2Digest,
        out_len: usize,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        let mut out = vec![0u8; out_len];
        kdf::openssl_derive_pbkdf2(password, salt, iterations, digest, &mut out)?;
        Ok(out)
    }

    fn argon2(
        &self,
        password: &[u8],
        salt: &[u8],
        params: Argon2Params,
        out_len: usize,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        // OpenSSL provider does not implement Argon2; defer to feature-gated RustCrypto implementation
        kdf::rustcrypto_derive_argon2(password, salt, params, out_len)
    }
}

/// RustCrypto-backed KDF provider for Argon2 (and PBKDF2 via OpenSSL helper for now)
pub struct RustCryptoKdfProvider;
impl KdfProvider for RustCryptoKdfProvider {
    fn pbkdf2(
        &self,
        password: &[u8],
        salt: &[u8],
        iterations: usize,
        digest: Pbkdf2Digest,
        out_len: usize,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        let mut out = vec![0u8; out_len];
        kdf::openssl_derive_pbkdf2(password, salt, iterations, digest, &mut out)?;
        Ok(out)
    }

    fn argon2(
        &self,
        password: &[u8],
        salt: &[u8],
        params: Argon2Params,
        out_len: usize,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        kdf::rustcrypto_derive_argon2(password, salt, params, out_len)
    }
}

pub struct OpenSslAsymProvider;
impl AsymProvider for OpenSslAsymProvider {
    fn encrypt(
        &self,
        alg: asym::AsymEncryptAlgorithm,
        public_key: &PublicKey,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError> {
        let pk: PKey<Public> = PKey::public_key_from_pem(public_key.as_pem())?;
        asym::openssl_asym_encrypt(alg, &pk, plaintext)
    }
    fn decrypt(
        &self,
        alg: asym::AsymEncryptAlgorithm,
        private_key: &PrivateKey,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError> {
        let sk: PKey<Private> = PKey::private_key_from_pem(private_key.as_pem())?;
        asym::openssl_asym_decrypt(alg, &sk, ciphertext)
    }
    fn sign(
        &self,
        alg: asym::AsymSignAlgorithm,
        private_key: &PrivateKey,
        msg: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError> {
        let sk: PKey<Private> = PKey::private_key_from_pem(private_key.as_pem())?;
        asym::openssl_asym_sign(alg, &sk, msg)
    }
    fn verify(
        &self,
        alg: asym::AsymSignAlgorithm,
        public_key: &PublicKey,
        msg: &[u8],
        sig: &[u8],
    ) -> Result<bool, AirframeCryptError> {
        let pk: PKey<Public> = PKey::public_key_from_pem(public_key.as_pem())?;
        asym::openssl_asym_verify(alg, &pk, msg, sig)
    }
    fn derive(
        &self,
        alg: asym::AsymKexAlgorithm,
        my_private: &PrivateKey,
        peer_public: &PublicKey,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        let sk: PKey<Private> = PKey::private_key_from_pem(my_private.as_pem())?;
        let pk: PKey<Public> = PKey::public_key_from_pem(peer_public.as_pem())?;
        asym::openssl_asym_derive(alg, &sk, &pk)
    }
}

pub struct OpenSslKeyWrapProvider;
impl KeyWrapProvider for OpenSslKeyWrapProvider {
    fn wrap_key(&self, kek: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, AirframeCryptError> {
        let wrapper = OpenSSLKeyWrapper::new(kek.to_vec())?;
        wrapper.wrap_key(plaintext)
    }
    fn unwrap_key(&self, kek: &[u8], wrapped: &[u8]) -> Result<Vec<u8>, AirframeCryptError> {
        let wrapper = OpenSSLKeyWrapper::new(kek.to_vec())?;
        wrapper.unwrap_key(wrapped)
    }
}

pub struct TotpRsProvider;
impl OtpProvider for TotpRsProvider {
    fn totp_generate_current(
        &self,
        alg: OtpAlgorithm,
        secret: &[u8],
        digits: u32,
        step: u64,
        skew: u8,
    ) -> Result<String, AirframeCryptError> {
        otp::totprs_totp_generate_current(alg, secret, digits, step, skew)
    }

    fn totp_verify_current(
        &self,
        alg: OtpAlgorithm,
        secret: &[u8],
        code: &str,
        digits: u32,
        step: u64,
        skew: u8,
    ) -> Result<bool, AirframeCryptError> {
        otp::totprs_totp_verify_current(alg, secret, code, digits, step, skew)
    }
}

#[cfg(test)]
mod software_suite_tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    #[test]
    fn software_suite_basic_hash_and_sym() {
        let suite = SoftwareCipherSuite::new();
        let d = suite.digest(DigestAlgorithm::Sha256, b"abc").unwrap();
        assert_eq!(
            hex(&d),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );

        let key = vec![0x11; 16];
        let nonce = vec![0x22; 12];
        let msg = b"hi";
        let ct = suite
            .sym_encrypt(SymmetricAlgorithm::AesGcm, &key, &nonce, msg, None)
            .unwrap();
        let pt = suite
            .sym_decrypt(SymmetricAlgorithm::AesGcm, &key, &nonce, &ct, None)
            .unwrap();
        assert_eq!(pt, msg);
    }

    #[cfg(feature = "argon2")]
    #[test]
    fn software_suite_argon2_basic() {
        let suite = SoftwareCipherSuite::new();
        use crate::kdf::{Argon2Params, Argon2Variant};
        let params = Argon2Params {
            variant: Argon2Variant::Id,
            ..Default::default()
        };
        let out = suite
            .argon2(b"password", b"0123456789ABCDEF", params, 32)
            .unwrap();
        assert_eq!(out.len(), 32);
        assert!(out.iter().any(|&b| b != 0));
    }

    #[test]
    fn software_suite_asym_sign_verify_with_air_keys() {
        let suite = SoftwareCipherSuite::new();
        let sk = asym::openssl_ed25519_generate().unwrap();
        let sk_pem = sk.private_key_to_pem_pkcs8().unwrap();
        let pk = PKey::public_key_from_pem(&sk.public_key_to_pem().unwrap()).unwrap();
        let pk_pem = pk.public_key_to_pem().unwrap();
        let air_sk = PrivateKey::from_pem(sk_pem);
        let air_pk = PublicKey::from_pem(pk_pem);

        let msg = b"hello";
        let sig = suite
            .asym_sign(asym::AsymSignAlgorithm::Ed25519, &air_sk, msg)
            .unwrap();
        assert!(suite
            .asym_verify(asym::AsymSignAlgorithm::Ed25519, &air_pk, msg, &sig)
            .unwrap());
    }

    #[test]
    fn software_suite_asym_encrypt_decrypt_with_air_keys() {
        let suite = SoftwareCipherSuite::new();
        let sk = asym::openssl_rsa_generate(2048).unwrap();
        let pk: PKey<Public> = PKey::public_key_from_pem(&sk.public_key_to_pem().unwrap()).unwrap();
        let sk_pem = sk.private_key_to_pem_pkcs8().unwrap();
        let pk_pem = pk.public_key_to_pem().unwrap();
        let air_sk = PrivateKey::from_pem(sk_pem);
        let air_pk = PublicKey::from_pem(pk_pem);

        let msg = b"secret";
        let ct = suite
            .asym_encrypt(asym::AsymEncryptAlgorithm::RsaOaepSha256, &air_pk, msg)
            .unwrap();
        let pt = suite
            .asym_decrypt(asym::AsymEncryptAlgorithm::RsaOaepSha256, &air_sk, &ct)
            .unwrap();
        assert_eq!(pt, msg);
    }

    #[test]
    fn software_suite_totp_roundtrip() {
        let suite = SoftwareCipherSuite::new();
        let secret = b"0123456789abcdef";
        let code = suite
            .totp_generate_current(OtpAlgorithm::Sha1, secret, 6, 30, 1)
            .unwrap();
        assert!(suite
            .totp_verify_current(OtpAlgorithm::Sha1, secret, &code, 6, 30, 1)
            .unwrap());
    }
}
