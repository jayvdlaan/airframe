use async_trait::async_trait;

use crate::error::AuditError;

/// Async cryptographic backend for audit log operations.
///
/// Implementations:
/// - `SoftwareAuditCrypto` (feature "software") -- uses airframe_crypt locally
/// - `NanokeyAuditCrypto` (in nanopass crate) -- delegates to Nanokey Ed25519 endpoints
#[async_trait]
pub trait AuditCrypto: Send + Sync {
    /// Compute SHA-256 digest of `data`. Returns hex-encoded hash.
    async fn digest(&self, data: &[u8]) -> Result<String, AuditError>;

    /// Sign a message (typically the entry_hash bytes).
    /// Returns base64-encoded signature.
    async fn sign(&self, message: &[u8]) -> Result<String, AuditError>;

    /// Verify a signature against a message.
    /// Returns true if valid.
    async fn verify(&self, message: &[u8], signature: &str) -> Result<bool, AuditError>;
}
