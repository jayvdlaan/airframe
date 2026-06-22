use crate::asym;
use crate::error::AirframeCryptError;
use crate::hash::DigestAlgorithm;
use crate::kdf::{Argon2Params, Pbkdf2Digest};
use crate::otp::OtpAlgorithm;
use crate::sym::SymmetricAlgorithm;
#[cfg(test)]
use openssl::pkey::{PKey, Private, Public};

use super::{
    AsymProvider, HashProvider, KdfProvider, KeyWrapProvider, OtpProvider, PrivateKey, PublicKey,
    RandomProvider, SymProvider,
};

#[derive(Default)]
pub struct ProviderCipherSuite {
    hash: Option<Box<dyn HashProvider>>,       // digest and hmac
    random: Option<Box<dyn RandomProvider>>,   // random bytes
    sym: Option<Box<dyn SymProvider>>,         // symmetric crypto
    kdf: Option<Box<dyn KdfProvider>>,         // key derivation
    keywrap: Option<Box<dyn KeyWrapProvider>>, // key wrap/unwrap
    asym: Option<Box<dyn AsymProvider>>,       // asymmetric (encrypt/decrypt/sign/verify/derive)
    otp: Option<Box<dyn OtpProvider>>,         // otp (totp)
}

impl ProviderCipherSuite {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_hash_provider<P>(mut self, provider: P) -> Self
    where
        P: HashProvider + 'static,
    {
        self.hash = Some(Box::new(provider));
        self
    }

    pub fn with_random_provider<P>(mut self, provider: P) -> Self
    where
        P: RandomProvider + 'static,
    {
        self.random = Some(Box::new(provider));
        self
    }

    pub fn with_sym_provider<P>(mut self, provider: P) -> Self
    where
        P: SymProvider + 'static,
    {
        self.sym = Some(Box::new(provider));
        self
    }

    pub fn with_kdf_provider<P>(mut self, provider: P) -> Self
    where
        P: KdfProvider + 'static,
    {
        self.kdf = Some(Box::new(provider));
        self
    }

    pub fn with_keywrap_provider<P>(mut self, provider: P) -> Self
    where
        P: KeyWrapProvider + 'static,
    {
        self.keywrap = Some(Box::new(provider));
        self
    }

    pub fn with_asym_provider<P>(mut self, provider: P) -> Self
    where
        P: AsymProvider + 'static,
    {
        self.asym = Some(Box::new(provider));
        self
    }

    pub fn with_otp_provider<P>(mut self, provider: P) -> Self
    where
        P: OtpProvider + 'static,
    {
        self.otp = Some(Box::new(provider));
        self
    }

    // Immediate action methods
    pub fn digest(&self, alg: DigestAlgorithm, data: &[u8]) -> Result<Vec<u8>, AirframeCryptError> {
        if let Some(p) = &self.hash {
            p.digest(alg, data)
        } else {
            Err(AirframeCryptError::UnsupportedAlgorithm)
        }
    }

    pub fn hmac(
        &self,
        alg: DigestAlgorithm,
        key: &[u8],
        data: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError> {
        if let Some(p) = &self.hash {
            p.hmac(alg, key, data)
        } else {
            Err(AirframeCryptError::UnsupportedAlgorithm)
        }
    }

    pub fn random_bytes(&self, len: usize) -> Result<Vec<u8>, AirframeCryptError> {
        if let Some(p) = &self.random {
            p.random_bytes(len)
        } else {
            Err(AirframeCryptError::UnsupportedAlgorithm)
        }
    }

    pub fn sym_encrypt(
        &self,
        alg: SymmetricAlgorithm,
        key: &[u8],
        iv_or_nonce: &[u8],
        plaintext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        if let Some(p) = &self.sym {
            p.encrypt(alg, key, iv_or_nonce, plaintext, aad)
        } else {
            Err(AirframeCryptError::UnsupportedAlgorithm)
        }
    }

    pub fn sym_decrypt(
        &self,
        alg: SymmetricAlgorithm,
        key: &[u8],
        iv_or_nonce: &[u8],
        ciphertext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        if let Some(p) = &self.sym {
            p.decrypt(alg, key, iv_or_nonce, ciphertext, aad)
        } else {
            Err(AirframeCryptError::UnsupportedAlgorithm)
        }
    }

    // KDF actions
    pub fn pbkdf2(
        &self,
        password: &[u8],
        salt: &[u8],
        iterations: usize,
        digest: Pbkdf2Digest,
        out_len: usize,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        if let Some(p) = &self.kdf {
            p.pbkdf2(password, salt, iterations, digest, out_len)
        } else {
            Err(AirframeCryptError::UnsupportedAlgorithm)
        }
    }

    pub fn argon2(
        &self,
        password: &[u8],
        salt: &[u8],
        params: Argon2Params,
        out_len: usize,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        if let Some(p) = &self.kdf {
            p.argon2(password, salt, params, out_len)
        } else {
            Err(AirframeCryptError::UnsupportedAlgorithm)
        }
    }

    // Key wrap actions
    pub fn wrap_key(&self, kek: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, AirframeCryptError> {
        if let Some(p) = &self.keywrap {
            p.wrap_key(kek, plaintext)
        } else {
            Err(AirframeCryptError::UnsupportedAlgorithm)
        }
    }
    pub fn unwrap_key(&self, kek: &[u8], wrapped: &[u8]) -> Result<Vec<u8>, AirframeCryptError> {
        if let Some(p) = &self.keywrap {
            p.unwrap_key(kek, wrapped)
        } else {
            Err(AirframeCryptError::UnsupportedAlgorithm)
        }
    }

    // OTP actions
    pub fn totp_generate_current(
        &self,
        alg: OtpAlgorithm,
        secret: &[u8],
        digits: u32,
        step: u64,
        skew: u8,
    ) -> Result<String, AirframeCryptError> {
        if let Some(p) = &self.otp {
            p.totp_generate_current(alg, secret, digits, step, skew)
        } else {
            Err(AirframeCryptError::UnsupportedAlgorithm)
        }
    }
    pub fn totp_verify_current(
        &self,
        alg: OtpAlgorithm,
        secret: &[u8],
        code: &str,
        digits: u32,
        step: u64,
        skew: u8,
    ) -> Result<bool, AirframeCryptError> {
        if let Some(p) = &self.otp {
            p.totp_verify_current(alg, secret, code, digits, step, skew)
        } else {
            Err(AirframeCryptError::UnsupportedAlgorithm)
        }
    }

    // Optional getters for advanced users who want to retrieve providers
    pub fn hash_provider(&self) -> Option<&dyn HashProvider> {
        self.hash.as_deref()
    }
    pub fn random_provider(&self) -> Option<&dyn RandomProvider> {
        self.random.as_deref()
    }
    pub fn sym_provider(&self) -> Option<&dyn SymProvider> {
        self.sym.as_deref()
    }
    pub fn asym_provider(&self) -> Option<&dyn AsymProvider> {
        self.asym.as_deref()
    }

    // Enum-based asymmetric convenience methods (provider-backed). Keys are the
    // backend-agnostic PEM wrappers — no OpenSSL `PKey` is exposed at this boundary.
    pub fn asym_encrypt(
        &self,
        alg: asym::AsymEncryptAlgorithm,
        public_key: &PublicKey,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError> {
        if let Some(p) = &self.asym {
            p.encrypt(alg, public_key, plaintext)
        } else {
            Err(AirframeCryptError::UnsupportedAlgorithm)
        }
    }

    pub fn asym_decrypt(
        &self,
        alg: asym::AsymEncryptAlgorithm,
        private_key: &PrivateKey,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError> {
        if let Some(p) = &self.asym {
            p.decrypt(alg, private_key, ciphertext)
        } else {
            Err(AirframeCryptError::UnsupportedAlgorithm)
        }
    }

    pub fn asym_sign(
        &self,
        alg: asym::AsymSignAlgorithm,
        private_key: &PrivateKey,
        msg: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError> {
        if let Some(p) = &self.asym {
            p.sign(alg, private_key, msg)
        } else {
            Err(AirframeCryptError::UnsupportedAlgorithm)
        }
    }

    pub fn asym_verify(
        &self,
        alg: asym::AsymSignAlgorithm,
        public_key: &PublicKey,
        msg: &[u8],
        sig: &[u8],
    ) -> Result<bool, AirframeCryptError> {
        if let Some(p) = &self.asym {
            p.verify(alg, public_key, msg, sig)
        } else {
            Err(AirframeCryptError::UnsupportedAlgorithm)
        }
    }

    pub fn asym_derive(
        &self,
        alg: asym::AsymKexAlgorithm,
        my_private: &PrivateKey,
        peer_public: &PublicKey,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        if let Some(p) = &self.asym {
            p.derive(alg, my_private, peer_public)
        } else {
            Err(AirframeCryptError::UnsupportedAlgorithm)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;
    use crate::kdf;

    // Test helpers: wrap an OpenSSL PKey in the backend-agnostic PEM wrapper that
    // the asym_* APIs now take (the APIs no longer accept a raw PKey).
    fn to_pub(pk: &PKey<Public>) -> PublicKey {
        PublicKey::from_pem(pk.public_key_to_pem().unwrap())
    }
    fn to_priv(sk: &PKey<Private>) -> PrivateKey {
        PrivateKey::from_pem(sk.private_key_to_pem_pkcs8().unwrap())
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    #[test]
    fn test_missing_providers_return_unsupported() {
        let suite = ProviderCipherSuite::new();
        assert!(matches!(
            suite.digest(DigestAlgorithm::Sha256, b"hi").unwrap_err(),
            AirframeCryptError::UnsupportedAlgorithm
        ));
        assert!(matches!(
            suite.hmac(DigestAlgorithm::Sha256, b"k", b"d").unwrap_err(),
            AirframeCryptError::UnsupportedAlgorithm
        ));
        assert!(matches!(
            suite.random_bytes(16).unwrap_err(),
            AirframeCryptError::UnsupportedAlgorithm
        ));
        assert!(matches!(
            suite
                .sym_encrypt(
                    SymmetricAlgorithm::AesGcm,
                    &[0u8; 16],
                    &[0u8; 12],
                    b"hi",
                    None
                )
                .unwrap_err(),
            AirframeCryptError::UnsupportedAlgorithm
        ));
        assert!(matches!(
            suite
                .totp_generate_current(OtpAlgorithm::Sha1, b"secret", 6, 30, 1)
                .unwrap_err(),
            AirframeCryptError::UnsupportedAlgorithm
        ));
        assert!(matches!(
            suite
                .totp_verify_current(OtpAlgorithm::Sha1, b"secret", "000000", 6, 30, 1)
                .unwrap_err(),
            AirframeCryptError::UnsupportedAlgorithm
        ));
        // Asymmetric operations should also be unsupported without a provider
        let sk = asym::openssl_rsa_generate(2048).unwrap();
        let pk: PKey<Public> = PKey::public_key_from_pem(&sk.public_key_to_pem().unwrap()).unwrap();
        assert!(matches!(
            suite
                .asym_sign(asym::AsymSignAlgorithm::RsaPssSha256, &to_priv(&sk), b"m")
                .unwrap_err(),
            AirframeCryptError::UnsupportedAlgorithm
        ));
        assert!(matches!(
            suite
                .asym_verify(
                    asym::AsymSignAlgorithm::RsaPssSha256,
                    &to_pub(&pk),
                    b"m",
                    b"sig"
                )
                .unwrap_err(),
            AirframeCryptError::UnsupportedAlgorithm
        ));
        let msg = b"enc";
        let ct_err = suite
            .asym_encrypt(asym::AsymEncryptAlgorithm::RsaOaepSha256, &to_pub(&pk), msg)
            .unwrap_err();
        assert!(matches!(ct_err, AirframeCryptError::UnsupportedAlgorithm));
    }

    #[test]
    fn test_openssl_hash_rng_sym_kdf_wrap_sign_suite() {
        let suite = ProviderCipherSuite::new()
            .with_hash_provider(OpenSslHashProvider)
            .with_random_provider(OpenSslRandomProvider)
            .with_sym_provider(OpenSslSymProvider)
            .with_kdf_provider(OpenSslKdfProvider)
            .with_keywrap_provider(OpenSslKeyWrapProvider)
            .with_asym_provider(OpenSslAsymProvider);

        // Validate digest output against known vector
        let out = suite
            .digest(
                DigestAlgorithm::Sha256,
                b"The quick brown fox jumps over the lazy dog",
            )
            .unwrap();
        assert_eq!(
            hex(&out),
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
        );

        // Validate HMAC output against RFC 4231 test case 1
        let key = vec![0x0b; 20];
        let mac = suite
            .hmac(DigestAlgorithm::Sha256, &key, b"Hi There")
            .unwrap();
        assert_eq!(
            hex(&mac),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );

        // Random should produce correct length and likely different consecutive values
        let r1 = suite.random_bytes(16).unwrap();
        let r2 = suite.random_bytes(16).unwrap();
        assert_eq!(r1.len(), 16);
        assert_eq!(r2.len(), 16);
        assert_ne!(r1, r2);

        // Symmetric AES-GCM roundtrip
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

        // KDF PBKDF2 derive and compare against direct helper
        let mut expected = vec![0u8; 32];
        kdf::openssl_derive_pbkdf2(
            b"password",
            b"salt",
            1000,
            Pbkdf2Digest::Sha256,
            &mut expected,
        )
        .unwrap();
        let got = suite
            .pbkdf2(b"password", b"salt", 1000, Pbkdf2Digest::Sha256, 32)
            .unwrap();
        assert_eq!(expected, got);

        // Key wrap/unwrap roundtrip
        let kek = vec![0xAA; 16];
        let plaintext_key = vec![0x55; 16];
        let wrapped = suite.wrap_key(&kek, &plaintext_key).unwrap();
        assert_eq!(wrapped.len(), plaintext_key.len() + 8);
        let unwrapped = suite.unwrap_key(&kek, &wrapped).unwrap();
        assert_eq!(unwrapped, plaintext_key);

        // Asymmetric via enum-based helpers
        let rsa_priv = asym::openssl_rsa_generate(2048).unwrap();
        let rsa_pub: PKey<Public> =
            PKey::public_key_from_pem(&rsa_priv.public_key_to_pem().unwrap()).unwrap();
        let msg = b"sign me";
        let sig_pss = suite
            .asym_sign(
                asym::AsymSignAlgorithm::RsaPssSha256,
                &to_priv(&rsa_priv),
                msg,
            )
            .unwrap();
        assert!(suite
            .asym_verify(
                asym::AsymSignAlgorithm::RsaPssSha256,
                &to_pub(&rsa_pub),
                msg,
                &sig_pss
            )
            .unwrap());
        let sig_pk = suite
            .asym_sign(
                asym::AsymSignAlgorithm::RsaPkcs1v15Sha256,
                &to_priv(&rsa_priv),
                msg,
            )
            .unwrap();
        assert!(suite
            .asym_verify(
                asym::AsymSignAlgorithm::RsaPkcs1v15Sha256,
                &to_pub(&rsa_pub),
                msg,
                &sig_pk
            )
            .unwrap());

        // Ed25519 sign/verify via enum
        let ed_priv = asym::openssl_ed25519_generate().unwrap();
        let ed_pub = asym::openssl_ed25519_public(&ed_priv).unwrap();
        let ed_sig = suite
            .asym_sign(
                asym::AsymSignAlgorithm::Ed25519,
                &to_priv(&ed_priv),
                b"hello",
            )
            .unwrap();
        assert!(suite
            .asym_verify(
                asym::AsymSignAlgorithm::Ed25519,
                &to_pub(&ed_pub),
                b"hello",
                &ed_sig
            )
            .unwrap());
    }

    #[test]
    fn test_missing_new_providers_return_unsupported() {
        let suite = ProviderCipherSuite::new();
        assert!(matches!(
            suite
                .pbkdf2(b"p", b"s", 1, Pbkdf2Digest::Sha256, 32)
                .unwrap_err(),
            AirframeCryptError::UnsupportedAlgorithm
        ));
        // Argon2 should also be unsupported when no KDF provider is configured
        use crate::kdf::{Argon2Params, Argon2Variant};
        let params = Argon2Params {
            variant: Argon2Variant::Id,
            ..Default::default()
        };
        assert!(matches!(
            suite
                .argon2(b"password", b"0123456789ABCDEF", params, 32)
                .unwrap_err(),
            AirframeCryptError::UnsupportedAlgorithm
        ));
        let kek = vec![0x11; 16];
        let pt = vec![0x22; 16];
        assert!(matches!(
            suite.wrap_key(&kek, &pt).unwrap_err(),
            AirframeCryptError::UnsupportedAlgorithm
        ));
    }

    #[cfg(feature = "argon2")]
    #[test]
    fn provider_suite_argon2_basic() {
        let suite = ProviderCipherSuite::new().with_kdf_provider(OpenSslKdfProvider);
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
    fn test_totp_provider_generate_verify_roundtrip() {
        let suite = ProviderCipherSuite::new().with_otp_provider(TotpRsProvider);
        let secret = b"0123456789abcdef"; // 16 bytes (128-bit) minimum per totp-rs
        let code = suite
            .totp_generate_current(OtpAlgorithm::Sha1, secret, 6, 30, 1)
            .unwrap();
        assert!(suite
            .totp_verify_current(OtpAlgorithm::Sha1, secret, &code, 6, 30, 1)
            .unwrap());
    }

    #[test]
    fn test_suite_enum_asym_helpers() {
        let suite = ProviderCipherSuite::new().with_asym_provider(OpenSslAsymProvider);

        // RSA OAEP encrypt/decrypt via enum
        let sk = asym::openssl_rsa_generate(2048).unwrap();
        let pk: PKey<Public> = PKey::public_key_from_pem(&sk.public_key_to_pem().unwrap()).unwrap();
        let msg = b"suite enum oaep";
        let ct = suite
            .asym_encrypt(asym::AsymEncryptAlgorithm::RsaOaepSha256, &to_pub(&pk), msg)
            .unwrap();
        let pt = suite
            .asym_decrypt(
                asym::AsymEncryptAlgorithm::RsaOaepSha256,
                &to_priv(&sk),
                &ct,
            )
            .unwrap();
        assert_eq!(pt, msg);

        // Enum sign/verify: RSA-PSS and PKCS#1 v1.5
        let msg2 = b"suite enum sign";
        let sig_pss = suite
            .asym_sign(asym::AsymSignAlgorithm::RsaPssSha256, &to_priv(&sk), msg2)
            .unwrap();
        assert!(suite
            .asym_verify(
                asym::AsymSignAlgorithm::RsaPssSha256,
                &to_pub(&pk),
                msg2,
                &sig_pss
            )
            .unwrap());
        let sig_pk = suite
            .asym_sign(
                asym::AsymSignAlgorithm::RsaPkcs1v15Sha256,
                &to_priv(&sk),
                msg2,
            )
            .unwrap();
        assert!(suite
            .asym_verify(
                asym::AsymSignAlgorithm::RsaPkcs1v15Sha256,
                &to_pub(&pk),
                msg2,
                &sig_pk
            )
            .unwrap());

        // Ed25519 via enum
        let ed_sk = asym::openssl_ed25519_generate().unwrap();
        let ed_pk: PKey<Public> =
            PKey::public_key_from_pem(&ed_sk.public_key_to_pem().unwrap()).unwrap();
        let sig_ed = suite
            .asym_sign(asym::AsymSignAlgorithm::Ed25519, &to_priv(&ed_sk), b"hello")
            .unwrap();
        assert!(suite
            .asym_verify(
                asym::AsymSignAlgorithm::Ed25519,
                &to_pub(&ed_pk),
                b"hello",
                &sig_ed
            )
            .unwrap());

        // X25519 derive via enum
        let a_sk = asym::openssl_x25519_generate().unwrap();
        let a_pk: PKey<Public> =
            PKey::public_key_from_pem(&a_sk.public_key_to_pem().unwrap()).unwrap();
        let b_sk = asym::openssl_x25519_generate().unwrap();
        let b_pk: PKey<Public> =
            PKey::public_key_from_pem(&b_sk.public_key_to_pem().unwrap()).unwrap();
        let a_ss = suite
            .asym_derive(
                asym::AsymKexAlgorithm::X25519,
                &to_priv(&a_sk),
                &to_pub(&b_pk),
            )
            .unwrap();
        let b_ss = suite
            .asym_derive(
                asym::AsymKexAlgorithm::X25519,
                &to_priv(&b_sk),
                &to_pub(&a_pk),
            )
            .unwrap();
        assert_eq!(a_ss, b_ss);
    }
}
