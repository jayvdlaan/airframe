mod composed;
mod providers;
mod software;

pub use composed::*;
pub use providers::*;
pub use software::*;

use crate::error::AirframeCryptError;

// Reuse existing algorithm enums and helpers from this crate
use crate::asym;
use crate::hash::DigestAlgorithm;
use crate::kdf::{Argon2Params, Pbkdf2Digest};
use crate::otp::OtpAlgorithm;
use crate::sym::SymmetricAlgorithm;

// Airframe-owned, backend-agnostic key wrappers for asymmetric operations
// Keys are expected to be provided in standard PEM encoding.
#[derive(Debug, Clone)]
pub struct PrivateKey {
    pem: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct PublicKey {
    pem: Vec<u8>,
}

impl PrivateKey {
    pub fn from_pem<P: AsRef<[u8]>>(pem: P) -> Self {
        Self {
            pem: pem.as_ref().to_vec(),
        }
    }
    pub fn as_pem(&self) -> &[u8] {
        &self.pem
    }
}

impl PublicKey {
    pub fn from_pem<P: AsRef<[u8]>>(pem: P) -> Self {
        Self {
            pem: pem.as_ref().to_vec(),
        }
    }
    pub fn as_pem(&self) -> &[u8] {
        &self.pem
    }
}

/// Backend-agnostic cryptographic interface.
/// This trait deliberately avoids depending on any specific crypto library types.
pub trait CipherSuite: Send + Sync {
    // Hashing
    fn digest(&self, alg: DigestAlgorithm, data: &[u8]) -> Result<Vec<u8>, AirframeCryptError>;
    fn hmac(
        &self,
        alg: DigestAlgorithm,
        key: &[u8],
        data: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError>;

    // Random
    fn random_bytes(&self, len: usize) -> Result<Vec<u8>, AirframeCryptError>;

    // Symmetric
    fn sym_encrypt(
        &self,
        alg: SymmetricAlgorithm,
        key: &[u8],
        iv_or_nonce: &[u8],
        plaintext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AirframeCryptError>;
    fn sym_decrypt(
        &self,
        alg: SymmetricAlgorithm,
        key: &[u8],
        iv_or_nonce: &[u8],
        ciphertext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AirframeCryptError>;

    // KDF
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

    // Key wrap (RFC3394)
    fn wrap_key(&self, kek: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, AirframeCryptError>;
    fn unwrap_key(&self, kek: &[u8], wrapped: &[u8]) -> Result<Vec<u8>, AirframeCryptError>;

    // Asymmetric (enum-dispatch)
    fn asym_encrypt(
        &self,
        alg: asym::AsymEncryptAlgorithm,
        public_key: &PublicKey,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError>;
    fn asym_decrypt(
        &self,
        alg: asym::AsymEncryptAlgorithm,
        private_key: &PrivateKey,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError>;
    fn asym_sign(
        &self,
        alg: asym::AsymSignAlgorithm,
        private_key: &PrivateKey,
        msg: &[u8],
    ) -> Result<Vec<u8>, AirframeCryptError>;
    fn asym_verify(
        &self,
        alg: asym::AsymSignAlgorithm,
        public_key: &PublicKey,
        msg: &[u8],
        sig: &[u8],
    ) -> Result<bool, AirframeCryptError>;
    fn asym_derive(
        &self,
        alg: asym::AsymKexAlgorithm,
        my_private: &PrivateKey,
        peer_public: &PublicKey,
    ) -> Result<Vec<u8>, AirframeCryptError>;

    // OTP (TOTP)
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
