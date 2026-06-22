//! Error types for bundle and share operations.

use thiserror::Error;

/// Errors from bundle structure operations.
#[derive(Debug, Error)]
pub enum BundleError {
    /// The bundle's `bundle_version` doesn't match what's expected.
    #[error("bundle_version mismatch: bundle={bundle}, share={share}")]
    VersionMismatch {
        /// Version on the bundle metadata.
        bundle: u32,
        /// Version stamped into the share's wrapper.
        share: u32,
    },

    /// A recovery constraint was not satisfied by the methods provided.
    #[error("recovery constraint not satisfied: {0}")]
    ConstraintFailed(String),
}

/// Errors from Shamir share-combining operations.
#[derive(Debug, Error)]
pub enum ShareError {
    /// Threshold parameters are invalid (k=0, k>n, or n=0).
    #[error("invalid threshold: k={k}, n={n}")]
    InvalidThreshold {
        /// The threshold k.
        k: u8,
        /// The total n.
        n: u8,
    },

    /// Too few shares provided to reconstruct the secret.
    #[error("insufficient shares: needed at least {needed}, got {provided}")]
    InsufficientShares {
        /// Threshold k.
        needed: u8,
        /// Number of shares actually provided.
        provided: usize,
    },

    /// A share could not be parsed.
    #[error("malformed share at index {index}")]
    MalformedShare {
        /// Position in the input list.
        index: usize,
    },

    /// Reconstruction succeeded but the result is the wrong size.
    #[error("reconstructed secret has wrong length: expected 32, got {got}")]
    WrongSecretLength {
        /// Length of the reconstructed bytes.
        got: usize,
    },

    /// A submitted share's bytes don't match the commitment recorded for it
    /// at split/enrollment time — the share is corrupt or tampered. Detected
    /// *before* combining, so a bad share is rejected (with attribution)
    /// instead of silently producing a wrong KEK.
    #[error("share commitment mismatch at index {index}")]
    CommitmentMismatch {
        /// Position in the input list of the offending share.
        index: usize,
    },

    /// The number of commitments supplied doesn't match the number of shares.
    #[error("commitment count {commitments} does not match share count {shares}")]
    CommitmentCountMismatch {
        /// Number of commitments supplied.
        commitments: usize,
        /// Number of shares supplied.
        shares: usize,
    },
}
