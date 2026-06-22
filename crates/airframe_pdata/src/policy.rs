#[cfg(feature = "compress")]
use std::sync::Arc;

#[cfg(feature = "compress")]
use airframe_compress::Compressor;

use airframe_crypt::hash::{openssl_digest, DigestAlgorithm};
use airframe_data::key::Key;

/// Compression policy for protected data. Performed on plaintext before encryption.
#[derive(Clone, Default)]
pub enum Compression {
    /// No compression.
    #[default]
    Disabled,
    /// Compress using a specific algorithm implementation (feature-gated).
    #[cfg(feature = "compress")]
    Algo(Arc<dyn Compressor>),
}

pub type Aad = Vec<u8>;

/// Strategy to derive AEAD AAD from contextual information.
///
/// Note: Airframe naming convention: the default/basic implementation is named "Basic".
pub trait AadPolicy: Send + Sync {
    /// Compose AAD bytes given namespace, logical key, and optional clear meta bytes (e.g., index).
    fn compose(&self, namespace: &str, key: &Key, clear_meta: Option<&[u8]>) -> Aad;
}

/// Basic AAD policy: binds namespace + key only.
#[derive(Clone, Default)]
pub struct BasicAadPolicy;

impl AadPolicy for BasicAadPolicy {
    fn compose(&self, namespace: &str, key: &Key, clear_meta: Option<&[u8]>) -> Aad {
        let mut out = Vec::new();
        if !namespace.is_empty() {
            out.extend_from_slice(b"ns=");
            out.extend_from_slice(namespace.as_bytes());
            out.push(b'|');
        }
        out.extend_from_slice(b"k=");
        out.extend_from_slice(key.as_str().as_bytes());
        // Fail-safe: if the caller supplies clear_meta they intend it to be
        // authenticated, so bind a hash of it into the AAD rather than silently
        // dropping it (which would leave the meta unauthenticated and tamperable).
        if let Some(m) = clear_meta {
            out.push(b'|');
            out.extend_from_slice(b"m:");
            if let Ok(d) = openssl_digest(DigestAlgorithm::Sha256, m) {
                for b in d {
                    out.extend_from_slice(format!("{:02x}", b).as_bytes());
                }
            } else {
                // Hashing should not fail; bind a constant tag so a failure can never
                // silently produce a meta-free AAD.
                out.extend_from_slice(b"sha256-error");
            }
        }
        out
    }
}

/// Indexed AAD policy: optionally includes a hash of provided clear_meta bytes to bind index to ciphertext.
#[derive(Clone, Debug)]
pub struct IndexedAadPolicy {
    pub include_meta_hash: bool,
}

impl IndexedAadPolicy {
    pub fn new(include_meta_hash: bool) -> Self {
        Self { include_meta_hash }
    }
}

impl AadPolicy for IndexedAadPolicy {
    fn compose(&self, namespace: &str, key: &Key, clear_meta: Option<&[u8]>) -> Aad {
        let mut out = Vec::new();
        if !namespace.is_empty() {
            out.extend_from_slice(b"ns=");
            out.extend_from_slice(namespace.as_bytes());
            out.push(b'|');
        }
        out.extend_from_slice(b"k=");
        out.extend_from_slice(key.as_str().as_bytes());
        if self.include_meta_hash {
            if let Some(m) = clear_meta {
                out.push(b'|');
                out.extend_from_slice(b"m:");
                if let Ok(d) = openssl_digest(DigestAlgorithm::Sha256, m) {
                    // hex-encode
                    for b in d {
                        out.extend_from_slice(format!("{:02x}", b).as_bytes());
                    }
                } else {
                    // If hashing fails unexpectedly, still bind a constant tag to prevent silent mismatch.
                    out.extend_from_slice(b"sha256-error");
                }
            }
        }
        out
    }
}
