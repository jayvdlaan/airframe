#[cfg(feature = "software")]
mod inner {
    use std::sync::Arc;

    use async_trait::async_trait;
    use base64::Engine;

    use airframe_crypt::asym::{
        openssl_ed25519_generate, openssl_ed25519_public, AsymSignAlgorithm,
    };
    use airframe_crypt::hash::DigestAlgorithm;
    use airframe_crypt::suite::{CipherSuite, PrivateKey, PublicKey};

    use crate::crypto::AuditCrypto;
    use crate::error::AuditError;

    /// Software-backed `AuditCrypto` using `airframe_crypt::CipherSuite`.
    ///
    /// Performs all crypto locally via OpenSSL -- suitable for tests and
    /// single-process deployments where Nanokey delegation is not required.
    #[deprecated(
        since = "0.6.0",
        note = "Use NanokeyAuditCrypto from libnanokey for production deployments. \
                SoftwareAuditCrypto performs crypto outside the Nanokey boundary."
    )]
    pub struct SoftwareAuditCrypto {
        suite: Arc<dyn CipherSuite>,
        private_key: PrivateKey,
        public_key: PublicKey,
    }

    impl SoftwareAuditCrypto {
        /// Create with an existing Ed25519 keypair.
        pub fn new(
            suite: Arc<dyn CipherSuite>,
            private_key: PrivateKey,
            public_key: PublicKey,
        ) -> Self {
            Self {
                suite,
                private_key,
                public_key,
            }
        }

        /// Generate a fresh Ed25519 keypair for testing.
        pub fn generate(suite: Arc<dyn CipherSuite>) -> Result<Self, AuditError> {
            let sk = openssl_ed25519_generate().map_err(|e| AuditError::Crypto(e.to_string()))?;
            let pk = openssl_ed25519_public(&sk).map_err(|e| AuditError::Crypto(e.to_string()))?;
            let private_key = PrivateKey::from_pem(
                sk.private_key_to_pem_pkcs8()
                    .map_err(|e| AuditError::Crypto(e.to_string()))?,
            );
            let public_key = PublicKey::from_pem(
                pk.public_key_to_pem()
                    .map_err(|e| AuditError::Crypto(e.to_string()))?,
            );
            Ok(Self {
                suite,
                private_key,
                public_key,
            })
        }
    }

    #[async_trait]
    impl AuditCrypto for SoftwareAuditCrypto {
        async fn digest(&self, data: &[u8]) -> Result<String, AuditError> {
            let hash = self
                .suite
                .digest(DigestAlgorithm::Sha256, data)
                .map_err(|e| AuditError::Crypto(e.to_string()))?;
            Ok(hex::encode(hash))
        }

        async fn sign(&self, message: &[u8]) -> Result<String, AuditError> {
            let sig = self
                .suite
                .asym_sign(AsymSignAlgorithm::Ed25519, &self.private_key, message)
                .map_err(|e| AuditError::Crypto(e.to_string()))?;
            Ok(base64::engine::general_purpose::STANDARD.encode(&sig))
        }

        async fn verify(&self, message: &[u8], signature: &str) -> Result<bool, AuditError> {
            let sig_bytes = base64::engine::general_purpose::STANDARD
                .decode(signature)
                .map_err(|e| AuditError::Crypto(e.to_string()))?;
            self.suite
                .asym_verify(
                    AsymSignAlgorithm::Ed25519,
                    &self.public_key,
                    message,
                    &sig_bytes,
                )
                .map_err(|e| AuditError::Crypto(e.to_string()))
        }
    }

    // hex encoding helper (avoid adding a crate dep for this)
    mod hex {
        const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

        pub fn encode(data: Vec<u8>) -> String {
            let mut s = String::with_capacity(data.len() * 2);
            for byte in &data {
                s.push(HEX_CHARS[(byte >> 4) as usize] as char);
                s.push(HEX_CHARS[(byte & 0xf) as usize] as char);
            }
            s
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use airframe_crypt::suite::SoftwareCipherSuite;

        #[tokio::test]
        async fn digest_produces_sha256_hex() {
            let suite: Arc<dyn CipherSuite> = Arc::new(SoftwareCipherSuite::new());
            let crypto = SoftwareAuditCrypto::generate(suite).unwrap();
            let hash = crypto.digest(b"hello").await.unwrap();
            // SHA-256 of "hello"
            assert_eq!(
                hash,
                "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
            );
        }

        #[tokio::test]
        async fn sign_verify_roundtrip() {
            let suite: Arc<dyn CipherSuite> = Arc::new(SoftwareCipherSuite::new());
            let crypto = SoftwareAuditCrypto::generate(suite).unwrap();
            let msg = b"audit entry hash";
            let sig = crypto.sign(msg).await.unwrap();
            assert!(crypto.verify(msg, &sig).await.unwrap());
            assert!(!crypto.verify(b"wrong", &sig).await.unwrap());
        }
    }
}

#[cfg(feature = "software")]
pub use inner::SoftwareAuditCrypto;
