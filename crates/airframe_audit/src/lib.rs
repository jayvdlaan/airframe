//! Tamper-evident, hash-chained audit logging for Airframe.
//!
//! `airframe_audit` is an append-only audit log whose entries are linked into a
//! SHA-256 hash chain. Each [`AuditEntry`] records a monotonic sequence number,
//! a timestamp, the previous entry's hash, and the hash of its own canonical
//! form — so any modification, reordering, or deletion of a past entry breaks
//! the chain and is caught on verification. Entries may also carry an Ed25519
//! signature over their hash, and a signed [`Checkpoint`] anchors the chain
//! against truncation or rollback.
//!
//! # Key pieces
//! - [`AuditChain`] / [`AuditChainConfig`] — append events (optionally signing);
//!   the coordinator over a crypto backend and a store.
//! - [`AuditEntry`] / [`AuditEvent`] — a chained log entry and the event it records.
//! - [`Checkpoint`] — a signed anchor; [`AuditChain::verify_against_checkpoint`]
//!   detects truncation/rollback below it.
//! - [`AuditStore`] / [`InMemoryAuditStore`] — persistence backend trait and an impl.
//! - [`AuditCrypto`] — hashing/signing backend; [`VerifyResult`] — verification outcome.
//! - [`AuditModule`] — Airframe module exposing the audit log (`cap:audit`).
//!
//! # Example
//! ```ignore
//! use airframe_audit::{AuditChain, AuditChainConfig, InMemoryAuditStore};
//! use std::sync::Arc;
//!
//! # async fn run(crypto: Arc<dyn airframe_audit::AuditCrypto>) -> Result<(), airframe_audit::AuditError> {
//! let chain = AuditChain::new(crypto, Arc::new(InMemoryAuditStore::new()), AuditChainConfig::default());
//! // chain.append(event).await?;  // each entry links to the previous via its hash
//! let result = chain.verify().await?;
//! assert!(result.valid);
//! # Ok(()) }
//! ```
pub mod chain;
pub mod crypto;
pub mod entry;
pub mod error;
pub mod module;
pub mod software;
pub mod store;
pub mod verify;

pub use chain::{AuditChain, AuditChainConfig, Checkpoint};
pub use crypto::AuditCrypto;
pub use entry::{AuditEntry, AuditEvent};
pub use error::AuditError;
pub use module::{AuditModule, ServiceRegistryAuditExt};
pub use store::{AuditStore, InMemoryAuditStore};
pub use verify::VerifyResult;

#[cfg(feature = "software")]
pub use software::SoftwareAuditCrypto;
