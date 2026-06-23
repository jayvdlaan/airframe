//! Cryptographic primitives and suites for Airframe — the framework's
//! cryptography boundary.
//!
//! `airframe_crypt` provides symmetric and asymmetric encryption, hashing,
//! key derivation, key wrapping, one-time passwords, signatures, and a CSPRNG,
//! built on OpenSSL. Application code is expected to go through this crate (or a
//! higher-level service that does) rather than reaching for crypto primitives
//! directly. The provider traits keep the backend behind a seam: public APIs
//! exchange backend-agnostic PEM key wrappers, not concrete OpenSSL types, so an
//! alternative backend can be slotted in.
//!
//! # Key pieces
//! - [`suite::CipherSuite`] — high-level entry point aggregating the operations.
//! - [`sym`] — symmetric encryption (AES-GCM, ChaCha20-Poly1305, …).
//! - [`asym`] — asymmetric keygen, sign/verify, encrypt/decrypt, key agreement.
//! - [`hash`] / [`kdf`] — digests and key derivation (PBKDF2, HKDF, Argon2).
//! - [`otp`] / [`rand`] — TOTP utilities and cryptographically secure randomness.
//! - [`CryptModule`] — Airframe module exposing a `CipherSuite` as `cap:crypt`.
//!
//! # Example
//! ```ignore
//! use airframe_crypt::suite::{CipherSuite, SoftwareCipherSuite};
//!
//! let suite = SoftwareCipherSuite::default();
//! let digest = suite.digest(/* algorithm */, b"message")?;
//! ```
pub mod asym;
pub mod envelope;
pub mod error;
mod gpg;
pub mod hash;
pub mod kdf;
pub mod module;
pub mod otp;
pub mod rand;
pub mod suite;
pub mod sym;

pub use module::{CryptModule, ServiceRegistryCryptExt};

// Unified algorithm identifier covering all algorithms used in this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlgorithmId {
    // Symmetric ciphers
    AesGcm,
    AesCbc,
    ChaCha20Poly1305,
    AesXts,
    CamelliaCbc,

    // Asymmetric encryption / signing / kex
    RsaOaepSha256,
    RsaPssSha256,
    RsaPkcs1v15Sha256,
    Ed25519,
    X25519,

    // Hash / HMAC digests
    Sha1,
    Sha256,
    Sha384,
    Sha512,
    Sha3_256,
    Sha3_384,
    Sha3_512,
    Blake2s256,
    Blake2b512,

    // KDFs
    Pbkdf2Sha256,
    Pbkdf2Sha512,
    Argon2id,

    // OTP
    TotpSha1,
    TotpSha256,
    TotpSha512,
}

impl AlgorithmId {
    pub fn as_str(self) -> &'static str {
        use AlgorithmId::*;
        match self {
            // Symmetric
            AesGcm => "aes-gcm",
            AesCbc => "aes-cbc",
            ChaCha20Poly1305 => "chacha20poly1305",
            AesXts => "aes-xts",
            CamelliaCbc => "camellia-cbc",

            // Asymmetric
            RsaOaepSha256 => "rsa-oaep-sha256",
            RsaPssSha256 => "rsa-pss-sha256",
            RsaPkcs1v15Sha256 => "rsa-pkcs1v15-sha256",
            Ed25519 => "ed25519",
            X25519 => "x25519",

            // Hash
            Sha1 => "sha1",
            Sha256 => "sha256",
            Sha384 => "sha384",
            Sha512 => "sha512",
            Sha3_256 => "sha3-256",
            Sha3_384 => "sha3-384",
            Sha3_512 => "sha3-512",
            Blake2s256 => "blake2s-256",
            Blake2b512 => "blake2b-512",

            // KDF
            Pbkdf2Sha256 => "pbkdf2-sha256",
            Pbkdf2Sha512 => "pbkdf2-sha512",
            Argon2id => "argon2id",

            // OTP
            TotpSha1 => "totp-sha1",
            TotpSha256 => "totp-sha256",
            TotpSha512 => "totp-sha512",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        let s = s.to_ascii_lowercase();
        Some(match s.as_str() {
            // Symmetric
            "aes-gcm" => AlgorithmId::AesGcm,
            "aes-cbc" => AlgorithmId::AesCbc,
            "chacha20poly1305" => AlgorithmId::ChaCha20Poly1305,
            "aes-xts" => AlgorithmId::AesXts,
            "camellia-cbc" => AlgorithmId::CamelliaCbc,

            // Asymmetric
            "rsa-oaep-sha256" => AlgorithmId::RsaOaepSha256,
            "rsa-pss-sha256" => AlgorithmId::RsaPssSha256,
            "rsa-pkcs1v15-sha256" => AlgorithmId::RsaPkcs1v15Sha256,
            "ed25519" => AlgorithmId::Ed25519,
            "x25519" => AlgorithmId::X25519,

            // Hash
            "sha1" => AlgorithmId::Sha1,
            "sha256" => AlgorithmId::Sha256,
            "sha384" => AlgorithmId::Sha384,
            "sha512" => AlgorithmId::Sha512,
            "sha3-256" => AlgorithmId::Sha3_256,
            "sha3-384" => AlgorithmId::Sha3_384,
            "sha3-512" => AlgorithmId::Sha3_512,
            "blake2s-256" => AlgorithmId::Blake2s256,
            "blake2b-512" => AlgorithmId::Blake2b512,

            // KDF
            "pbkdf2-sha256" => AlgorithmId::Pbkdf2Sha256,
            "pbkdf2-sha512" => AlgorithmId::Pbkdf2Sha512,
            "argon2id" => AlgorithmId::Argon2id,

            // OTP
            "totp-sha1" => AlgorithmId::TotpSha1,
            "totp-sha256" => AlgorithmId::TotpSha256,
            "totp-sha512" => AlgorithmId::TotpSha512,

            _ => return None,
        })
    }
}

// Conversions from local enums
impl From<sym::SymmetricAlgorithm> for AlgorithmId {
    fn from(a: sym::SymmetricAlgorithm) -> Self {
        match a {
            sym::SymmetricAlgorithm::AesGcm => AlgorithmId::AesGcm,
            sym::SymmetricAlgorithm::AesCbc => AlgorithmId::AesCbc,
            sym::SymmetricAlgorithm::ChaCha20Poly1305 => AlgorithmId::ChaCha20Poly1305,
            sym::SymmetricAlgorithm::AesXts => AlgorithmId::AesXts,
            sym::SymmetricAlgorithm::CamelliaCbc => AlgorithmId::CamelliaCbc,
        }
    }
}

impl From<asym::AsymEncryptAlgorithm> for AlgorithmId {
    fn from(a: asym::AsymEncryptAlgorithm) -> Self {
        match a {
            asym::AsymEncryptAlgorithm::RsaOaepSha256 => AlgorithmId::RsaOaepSha256,
        }
    }
}

impl From<asym::AsymSignAlgorithm> for AlgorithmId {
    fn from(a: asym::AsymSignAlgorithm) -> Self {
        match a {
            asym::AsymSignAlgorithm::RsaPssSha256 => AlgorithmId::RsaPssSha256,
            asym::AsymSignAlgorithm::RsaPkcs1v15Sha256 => AlgorithmId::RsaPkcs1v15Sha256,
            asym::AsymSignAlgorithm::Ed25519 => AlgorithmId::Ed25519,
        }
    }
}

impl From<asym::AsymKexAlgorithm> for AlgorithmId {
    fn from(a: asym::AsymKexAlgorithm) -> Self {
        match a {
            asym::AsymKexAlgorithm::X25519 => AlgorithmId::X25519,
        }
    }
}

impl From<hash::DigestAlgorithm> for AlgorithmId {
    fn from(a: hash::DigestAlgorithm) -> Self {
        match a {
            hash::DigestAlgorithm::Sha1 => AlgorithmId::Sha1,
            hash::DigestAlgorithm::Sha256 => AlgorithmId::Sha256,
            hash::DigestAlgorithm::Sha384 => AlgorithmId::Sha384,
            hash::DigestAlgorithm::Sha512 => AlgorithmId::Sha512,
            hash::DigestAlgorithm::Sha3_256 => AlgorithmId::Sha3_256,
            hash::DigestAlgorithm::Sha3_384 => AlgorithmId::Sha3_384,
            hash::DigestAlgorithm::Sha3_512 => AlgorithmId::Sha3_512,
            hash::DigestAlgorithm::Blake2s256 => AlgorithmId::Blake2s256,
            hash::DigestAlgorithm::Blake2b512 => AlgorithmId::Blake2b512,
        }
    }
}

impl From<kdf::Pbkdf2Digest> for AlgorithmId {
    fn from(a: kdf::Pbkdf2Digest) -> Self {
        match a {
            kdf::Pbkdf2Digest::Sha256 => AlgorithmId::Pbkdf2Sha256,
            kdf::Pbkdf2Digest::Sha512 => AlgorithmId::Pbkdf2Sha512,
        }
    }
}

impl From<otp::OtpAlgorithm> for AlgorithmId {
    fn from(a: otp::OtpAlgorithm) -> Self {
        match a {
            otp::OtpAlgorithm::Sha1 => AlgorithmId::TotpSha1,
            otp::OtpAlgorithm::Sha256 => AlgorithmId::TotpSha256,
            otp::OtpAlgorithm::Sha512 => AlgorithmId::TotpSha512,
        }
    }
}

// Inverse conversions to reduce ad-hoc mapping at call sites
impl core::convert::TryFrom<AlgorithmId> for sym::SymmetricAlgorithm {
    type Error = crate::error::AirframeCryptError;
    fn try_from(a: AlgorithmId) -> Result<Self, Self::Error> {
        Ok(match a {
            AlgorithmId::AesGcm => sym::SymmetricAlgorithm::AesGcm,
            AlgorithmId::AesCbc => sym::SymmetricAlgorithm::AesCbc,
            AlgorithmId::ChaCha20Poly1305 => sym::SymmetricAlgorithm::ChaCha20Poly1305,
            AlgorithmId::AesXts => sym::SymmetricAlgorithm::AesXts,
            AlgorithmId::CamelliaCbc => sym::SymmetricAlgorithm::CamelliaCbc,
            _ => return Err(crate::error::AirframeCryptError::UnsupportedAlgorithm),
        })
    }
}

impl core::convert::TryFrom<AlgorithmId> for kdf::Pbkdf2Digest {
    type Error = crate::error::AirframeCryptError;
    fn try_from(a: AlgorithmId) -> Result<Self, Self::Error> {
        Ok(match a {
            AlgorithmId::Pbkdf2Sha256 => kdf::Pbkdf2Digest::Sha256,
            AlgorithmId::Pbkdf2Sha512 => kdf::Pbkdf2Digest::Sha512,
            _ => return Err(crate::error::AirframeCryptError::UnsupportedAlgorithm),
        })
    }
}

impl core::convert::TryFrom<AlgorithmId> for hash::DigestAlgorithm {
    type Error = crate::error::AirframeCryptError;
    fn try_from(a: AlgorithmId) -> Result<Self, Self::Error> {
        Ok(match a {
            AlgorithmId::Sha1 => hash::DigestAlgorithm::Sha1,
            AlgorithmId::Sha256 => hash::DigestAlgorithm::Sha256,
            AlgorithmId::Sha384 => hash::DigestAlgorithm::Sha384,
            AlgorithmId::Sha512 => hash::DigestAlgorithm::Sha512,
            AlgorithmId::Sha3_256 => hash::DigestAlgorithm::Sha3_256,
            AlgorithmId::Sha3_384 => hash::DigestAlgorithm::Sha3_384,
            AlgorithmId::Sha3_512 => hash::DigestAlgorithm::Sha3_512,
            AlgorithmId::Blake2s256 => hash::DigestAlgorithm::Blake2s256,
            AlgorithmId::Blake2b512 => hash::DigestAlgorithm::Blake2b512,
            _ => return Err(crate::error::AirframeCryptError::UnsupportedAlgorithm),
        })
    }
}

impl core::convert::TryFrom<AlgorithmId> for otp::OtpAlgorithm {
    type Error = crate::error::AirframeCryptError;
    fn try_from(a: AlgorithmId) -> Result<Self, Self::Error> {
        Ok(match a {
            AlgorithmId::TotpSha1 => otp::OtpAlgorithm::Sha1,
            AlgorithmId::TotpSha256 => otp::OtpAlgorithm::Sha256,
            AlgorithmId::TotpSha512 => otp::OtpAlgorithm::Sha512,
            _ => return Err(crate::error::AirframeCryptError::UnsupportedAlgorithm),
        })
    }
}

impl core::convert::TryFrom<AlgorithmId> for asym::AsymEncryptAlgorithm {
    type Error = crate::error::AirframeCryptError;
    fn try_from(a: AlgorithmId) -> Result<Self, Self::Error> {
        Ok(match a {
            AlgorithmId::RsaOaepSha256 => asym::AsymEncryptAlgorithm::RsaOaepSha256,
            _ => return Err(crate::error::AirframeCryptError::UnsupportedAlgorithm),
        })
    }
}

impl core::convert::TryFrom<AlgorithmId> for asym::AsymSignAlgorithm {
    type Error = crate::error::AirframeCryptError;
    fn try_from(a: AlgorithmId) -> Result<Self, Self::Error> {
        Ok(match a {
            AlgorithmId::RsaPssSha256 => asym::AsymSignAlgorithm::RsaPssSha256,
            AlgorithmId::RsaPkcs1v15Sha256 => asym::AsymSignAlgorithm::RsaPkcs1v15Sha256,
            AlgorithmId::Ed25519 => asym::AsymSignAlgorithm::Ed25519,
            _ => return Err(crate::error::AirframeCryptError::UnsupportedAlgorithm),
        })
    }
}

impl core::convert::TryFrom<AlgorithmId> for asym::AsymKexAlgorithm {
    type Error = crate::error::AirframeCryptError;
    fn try_from(a: AlgorithmId) -> Result<Self, Self::Error> {
        Ok(match a {
            AlgorithmId::X25519 => asym::AsymKexAlgorithm::X25519,
            _ => return Err(crate::error::AirframeCryptError::UnsupportedAlgorithm),
        })
    }
}

// Serde support for AlgorithmId using canonical strings
impl serde::Serialize for AlgorithmId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for AlgorithmId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        AlgorithmId::from_str(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown algorithm: {}", s)))
    }
}
