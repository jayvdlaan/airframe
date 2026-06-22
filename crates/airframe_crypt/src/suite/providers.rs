use super::{PrivateKey, PublicKey};
use crate::asym;
use crate::error::AirframeCryptError;
use crate::hash::DigestAlgorithm;
use crate::kdf::{Argon2Params, Pbkdf2Digest};
use crate::otp::OtpAlgorithm;
use crate::sym::SymmetricAlgorithm;

/// Provider trait for hashing (digest and HMAC)
pub trait HashProvider: Send + Sync {
    fn digest(&self, alg: DigestAlgorithm, data: &[u8]) -> Result<Vec<u8>, AirframeCryptError>;
    fn hmac(
        &self,
        alg: DigestAlgorithm,
        key: &[u8],
        data: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError>;
}

/// Provider trait for random byte generation
pub trait RandomProvider: Send + Sync {
    fn random_bytes(&self, len: usize) -> Result<Vec<u8>, AirframeCryptError>;
}

/// CipherSuite aggregates optional providers/callbacks for cryptographic functions.
/// Users can configure only the functions they need. Methods return
/// AirframeCryptError::UnsupportedAlgorithm when the corresponding provider is not set.
/// Provider trait for symmetric crypto (one-shot APIs)
pub trait SymProvider: Send + Sync {
    fn encrypt(
        &self,
        alg: SymmetricAlgorithm,
        key: &[u8],
        iv_or_nonce: &[u8],
        plaintext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AirframeCryptError>;

    fn decrypt(
        &self,
        alg: SymmetricAlgorithm,
        key: &[u8],
        iv_or_nonce: &[u8],
        ciphertext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AirframeCryptError>;
}

// KDF provider: PBKDF2 facade
pub trait KdfProvider: Send + Sync {
    fn pbkdf2(
        &self,
        password: &[u8],
        salt: &[u8],
        iterations: usize,
        digest: Pbkdf2Digest,
        out_len: usize,
    ) -> Result<Vec<u8>, AirframeCryptError>;

    fn argon2(
        &self,
        password: &[u8],
        salt: &[u8],
        params: Argon2Params,
        out_len: usize,
    ) -> Result<Vec<u8>, AirframeCryptError>;
}

// Asymmetric provider: generic entry points using enums for algorithms.
//
// Keys are passed as the backend-agnostic PEM wrappers (`PublicKey`/`PrivateKey`),
// NOT a concrete backend type — so an alternative (e.g. RustCrypto) provider can
// implement this trait without depending on OpenSSL's `PKey`.
pub trait AsymProvider: Send + Sync {
    fn encrypt(
        &self,
        alg: asym::AsymEncryptAlgorithm,
        public_key: &PublicKey,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError>;
    fn decrypt(
        &self,
        alg: asym::AsymEncryptAlgorithm,
        private_key: &PrivateKey,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError>;
    fn sign(
        &self,
        alg: asym::AsymSignAlgorithm,
        private_key: &PrivateKey,
        msg: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError>;
    fn verify(
        &self,
        alg: asym::AsymSignAlgorithm,
        public_key: &PublicKey,
        msg: &[u8],
        sig: &[u8],
    ) -> Result<bool, AirframeCryptError>;
    fn derive(
        &self,
        alg: asym::AsymKexAlgorithm,
        my_private: &PrivateKey,
        peer_public: &PublicKey,
    ) -> Result<Vec<u8>, AirframeCryptError>;
}

// Key wrapping provider using RFC3394 AES key wrap
pub trait KeyWrapProvider: Send + Sync {
    fn wrap_key(&self, kek: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, AirframeCryptError>;
    fn unwrap_key(&self, kek: &[u8], wrapped: &[u8]) -> Result<Vec<u8>, AirframeCryptError>;
}

// OTP provider trait and default totp-rs-based implementation
pub trait OtpProvider: Send + Sync {
    fn totp_generate_current(
        &self,
        alg: OtpAlgorithm,
        secret: &[u8],
        digits: u32,
        step: u64,
        skew: u8,
    ) -> Result<String, AirframeCryptError>;

    fn totp_verify_current(
        &self,
        alg: OtpAlgorithm,
        secret: &[u8],
        code: &str,
        digits: u32,
        step: u64,
        skew: u8,
    ) -> Result<bool, AirframeCryptError>;
}
