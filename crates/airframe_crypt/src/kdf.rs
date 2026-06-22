use crate::error::AirframeCryptError;
use openssl::hash::MessageDigest;
use openssl::pkcs5::pbkdf2_hmac;

/// Argon2 algorithm family variants.
///
/// Prefer Argon2id for password-based key derivation (recommended by RFC 9106).
#[derive(Debug, Clone, Copy)]
pub enum Argon2Variant {
    /// Argon2id (hybrid of i and d) — preferred for most use cases.
    Id,
    /// Argon2i — optimized against side-channel attacks; less GPU-resistant.
    I,
    /// Argon2d — faster, but more susceptible to side channels; avoid for passwords.
    D,
}

#[derive(Debug, Clone, Copy)]
pub struct Argon2Params {
    /// Algorithm variant to use. Prefer Argon2Variant::Id for password-based KDFs.
    pub variant: Argon2Variant,
    /// Memory cost in KiB (e.g., 64*1024 for 64 MiB). Enforced minimum: 8192 (8 MiB).
    pub m_cost_kib: u32,
    /// Time cost (iterations). Minimum: 1.
    pub t_cost: u32,
    /// Degree of parallelism (lanes). Minimum: 1.
    pub p_cost: u32,
    /// Argon2 version, e.g., 0x13 for v1.3 (RFC 9106). Other values map to v1.3.
    pub version: u32,
}

impl Default for Argon2Params {
    fn default() -> Self {
        Self {
            variant: Argon2Variant::Id,
            m_cost_kib: 64 * 1024, // 64 MiB
            t_cost: 3,
            p_cost: 1,
            version: 0x13, // v1.3 (RFC 9106)
        }
    }
}

/// Feature-gated Argon2 derivation. When feature is disabled, returns UnsupportedAlgorithm.
#[cfg_attr(not(feature = "argon2"), allow(unused_variables))]
pub fn rustcrypto_derive_argon2(
    password: &[u8],
    salt: &[u8],
    params: Argon2Params,
    out_len: usize,
) -> Result<Vec<u8>, AirframeCryptError> {
    #[cfg(feature = "argon2")]
    {
        use argon2::{Algorithm, Argon2, Params, Version};

        // Validate parameters
        if salt.len() < 16 {
            return Err(AirframeCryptError::InvalidParameters(
                "salt must be at least 16 bytes".into(),
            ));
        }
        if out_len == 0 || out_len > 64 {
            return Err(AirframeCryptError::InvalidParameters(
                "out_len must be in 1..=64 bytes".into(),
            ));
        }
        let m = params.m_cost_kib.max(8 * 1024); // enforce minimum 8 MiB
        if params.m_cost_kib < 8 * 1024 {
            return Err(AirframeCryptError::InvalidParameters(
                "m_cost_kib must be at least 8192 (8 MiB)".into(),
            ));
        }
        if params.t_cost < 1 {
            return Err(AirframeCryptError::InvalidParameters(
                "t_cost must be >= 1".into(),
            ));
        }
        if params.p_cost < 1 {
            return Err(AirframeCryptError::InvalidParameters(
                "p_cost must be >= 1".into(),
            ));
        }

        let alg = match params.variant {
            Argon2Variant::Id => Algorithm::Argon2id,
            Argon2Variant::I => Algorithm::Argon2i,
            Argon2Variant::D => Algorithm::Argon2d,
        };
        let ver = match params.version {
            0x13 => Version::V0x13,
            _ => Version::V0x13,
        };
        let p = Params::new(m, params.t_cost, params.p_cost, Some(out_len))
            .map_err(|e| AirframeCryptError::InvalidParameters(format!("argon2 params: {e}")))?;
        let a2 = Argon2::new(alg, ver, p);
        let mut out = vec![0u8; out_len];
        a2.hash_password_into(password, salt, &mut out)
            .map_err(|e| AirframeCryptError::InvalidParameters(format!("argon2 derive: {e}")))?;
        Ok(out)
    }
    #[cfg(not(feature = "argon2"))]
    {
        Err(AirframeCryptError::UnsupportedAlgorithm)
    }
}

pub trait KeyDeriver {
    fn derive_key(
        password: &[u8],
        salt: &[u8],
        iterations: usize,
        derived_key: &mut [u8],
    ) -> Result<(), AirframeCryptError>;
}

pub struct OpenSSLPBKDF2;

impl KeyDeriver for OpenSSLPBKDF2 {
    fn derive_key(
        password: &[u8],
        salt: &[u8],
        iterations: usize,
        derived_key: &mut [u8],
    ) -> Result<(), AirframeCryptError> {
        pbkdf2_hmac(
            password,
            salt,
            iterations,
            MessageDigest::sha256(),
            derived_key,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Pbkdf2Digest {
    Sha256,
    Sha512,
}

impl Pbkdf2Digest {
    fn to_md(self) -> MessageDigest {
        match self {
            Pbkdf2Digest::Sha256 => MessageDigest::sha256(),
            Pbkdf2Digest::Sha512 => MessageDigest::sha512(),
        }
    }
}

/// Derive key using PBKDF2-HMAC with selectable digest (SHA-256 or SHA-512).
pub fn openssl_derive_pbkdf2(
    password: &[u8],
    salt: &[u8],
    iterations: usize,
    digest: Pbkdf2Digest,
    out_key: &mut [u8],
) -> Result<(), AirframeCryptError> {
    pbkdf2_hmac(password, salt, iterations, digest.to_md(), out_key)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pbkdf2_sha256_vs_sha512_different() {
        let pwd = b"password";
        let salt = b"salt";
        let iters = 1000;
        let mut k256 = [0u8; 32];
        let mut k512 = [0u8; 32];
        openssl_derive_pbkdf2(pwd, salt, iters, Pbkdf2Digest::Sha256, &mut k256).unwrap();
        openssl_derive_pbkdf2(pwd, salt, iters, Pbkdf2Digest::Sha512, &mut k512).unwrap();
        assert_ne!(k256, k512);
        assert_ne!(k256, [0u8; 32]);
        assert_ne!(k512, [0u8; 32]);
    }

    #[test]
    fn test_pbkdf2_sha512_long_output() {
        let pwd = b"password";
        let salt = b"salt";
        let mut out = [0u8; 64];
        openssl_derive_pbkdf2(pwd, salt, 2000, Pbkdf2Digest::Sha512, &mut out).unwrap();
        assert_ne!(out, [0u8; 64]);
    }

    #[test]
    fn test_derive_key_basic() {
        // Test basic key derivation
        let password = b"password";
        let salt = b"salt";
        let iterations = 1000;
        let mut derived_key = [0u8; 32]; // 256 bits

        let result = OpenSSLPBKDF2::derive_key(password, salt, iterations, &mut derived_key);
        assert!(result.is_ok());

        // The derived key should not be all zeros
        assert_ne!(derived_key, [0u8; 32]);
    }

    #[test]
    fn test_derive_key_different_passwords() {
        // Test that different passwords produce different keys
        let salt = b"same_salt";
        let iterations = 1000;
        let mut key1 = [0u8; 32];
        let mut key2 = [0u8; 32];

        let _ = OpenSSLPBKDF2::derive_key(b"password1", salt, iterations, &mut key1);
        let _ = OpenSSLPBKDF2::derive_key(b"password2", salt, iterations, &mut key2);

        // The keys should be different
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_derive_key_different_salts() {
        // Test that different salts produce different keys
        let password = b"same_password";
        let iterations = 1000;
        let mut key1 = [0u8; 32];
        let mut key2 = [0u8; 32];

        let _ = OpenSSLPBKDF2::derive_key(password, b"salt1", iterations, &mut key1);
        let _ = OpenSSLPBKDF2::derive_key(password, b"salt2", iterations, &mut key2);

        // The keys should be different
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_derive_key_different_iterations() {
        // Test that different iteration counts produce different keys
        let password = b"same_password";
        let salt = b"same_salt";
        let mut key1 = [0u8; 32];
        let mut key2 = [0u8; 32];

        let _ = OpenSSLPBKDF2::derive_key(password, salt, 1000, &mut key1);
        let _ = OpenSSLPBKDF2::derive_key(password, salt, 2000, &mut key2);

        // The keys should be different
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_derive_key_consistency() {
        // Test that the same inputs produce the same key
        let password = b"password";
        let salt = b"salt";
        let iterations = 1000;
        let mut key1 = [0u8; 32];
        let mut key2 = [0u8; 32];

        let _ = OpenSSLPBKDF2::derive_key(password, salt, iterations, &mut key1);
        let _ = OpenSSLPBKDF2::derive_key(password, salt, iterations, &mut key2);

        // The keys should be identical
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_derive_key_empty_password() {
        // Test with an empty password
        let password = b"";
        let salt = b"salt";
        let iterations = 1000;
        let mut derived_key = [0u8; 32];

        let result = OpenSSLPBKDF2::derive_key(password, salt, iterations, &mut derived_key);
        assert!(result.is_ok());

        // The derived key should not be all zeros
        assert_ne!(derived_key, [0u8; 32]);
    }

    #[test]
    fn test_derive_key_empty_salt() {
        // Test with an empty salt
        let password = b"password";
        let salt = b"";
        let iterations = 1000;
        let mut derived_key = [0u8; 32];

        let result = OpenSSLPBKDF2::derive_key(password, salt, iterations, &mut derived_key);
        assert!(result.is_ok());

        // The derived key should not be all zeros
        assert_ne!(derived_key, [0u8; 32]);
    }

    #[test]
    fn test_derive_key_zero_iterations() {
        // Test with zero iterations
        // Note: OpenSSL's PBKDF2 implementation rejects zero iterations
        // as it doesn't provide any security benefit
        let password = b"password";
        let salt = b"salt";
        let iterations = 0;
        let mut derived_key = [0u8; 32];

        let result = OpenSSLPBKDF2::derive_key(password, salt, iterations, &mut derived_key);
        assert!(result.is_err());

        // The error should be an OpenSSLError
        if let Err(err) = result {
            match err {
                AirframeCryptError::OpenSSLError(_) => {} // Expected error type
                _ => panic!("Expected OpenSSLError, got {:?}", err),
            }
        }
    }

    #[test]
    fn test_derive_key_different_output_lengths() {
        // Test different output lengths
        let password = b"password";
        let salt = b"salt";
        let iterations = 1000;
        let mut short_key = [0u8; 16];
        let mut long_key = [0u8; 64];

        let short_result = OpenSSLPBKDF2::derive_key(password, salt, iterations, &mut short_key);
        let long_result = OpenSSLPBKDF2::derive_key(password, salt, iterations, &mut long_key);

        assert!(short_result.is_ok());
        assert!(long_result.is_ok());

        // The derived keys should not be all zeros
        assert_ne!(short_key, [0u8; 16]);
        assert_ne!(long_key, [0u8; 64]);
    }
}

#[cfg(all(test, feature = "argon2"))]
mod argon2_tests {
    use super::*;

    #[test]
    fn argon2id_basic_derivation() {
        let params = Argon2Params {
            variant: Argon2Variant::Id,
            ..Default::default()
        };
        let out =
            crate::kdf::rustcrypto_derive_argon2(b"password", b"0123456789ABCDEF", params, 32)
                .expect("derive ok");
        assert_eq!(out.len(), 32);
        assert!(out.iter().any(|&b| b != 0));
    }

    #[test]
    fn argon2id_short_salt_rejected() {
        let params = Argon2Params {
            variant: Argon2Variant::Id,
            ..Default::default()
        };
        let err = crate::kdf::rustcrypto_derive_argon2(b"password", b"short", params, 32)
            .err()
            .expect("error");
        match err {
            AirframeCryptError::InvalidParameters(_) => {}
            other => panic!("unexpected error: {:?}", other),
        }
    }
}
