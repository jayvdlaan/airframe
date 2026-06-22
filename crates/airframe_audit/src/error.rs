use airframe_core::error::ErrorRange;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("chain integrity violation at sequence {seq}: {detail}")]
    ChainIntegrity { seq: u64, detail: String },

    #[error("signature verification failed at sequence {seq}")]
    SignatureInvalid { seq: u64 },

    #[error("store error: {0}")]
    Store(String),

    #[error("crypto error: {0}")]
    Crypto(String),

    #[error("entry not found: seq={0}")]
    NotFound(u64),

    #[error("chain is empty")]
    EmptyChain,
}

impl AuditError {
    pub fn error_range() -> ErrorRange {
        ErrorRange::Audit
    }
}
