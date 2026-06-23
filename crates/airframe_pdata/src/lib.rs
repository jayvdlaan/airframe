//! Protected-at-rest data layer for Airframe — an AEAD pipeline with correct
//! transform ordering, built on `airframe_data`.
//!
//! `airframe_pdata` protects data at rest by composing serialization, optional
//! compression, and AEAD encryption in the *correct order*, then persisting
//! through any `airframe_data` backend or cache. It offers both bytes-level and
//! typed repositories.
//!
//! The pipeline ordering is the crate's core invariant:
//! - **Write**: serialize → (optional) compress → encrypt (AEAD) → persist
//! - **Read**: load → decrypt → (optional) decompress → deserialize
//!
//! Compressing *before* encrypting (never the reverse) avoids the classic
//! "compress ciphertext" layering pitfall.
//!
//! # Key pieces
//! - [`bytes`] — bytes-level protected repository.
//! - [`typed`] — typed protected repository over a codec.
//! - [`builder`] — assemble a pipeline from a codec, cache backend, and policy.
//! - [`context`] / [`policy`] — AEAD context (AAD binding) and protection policy.
//! - [`AirframePdataError`] — the crate error type.
//!
//! # Usage
//!
//! Use [`builder`] to assemble a protected repository from a codec, an
//! `airframe_data` cache/backend, and a protection policy, then put/get values —
//! serialization, optional compression, AEAD encryption, and the correct
//! transform ordering are applied for you. See the crate's `examples/` for
//! runnable samples.
pub mod builder;
pub mod bytes;
pub mod context;
pub mod error;
pub mod module;
pub mod policy;
pub mod secrets;
pub mod typed;

pub use error::{AirframePdataError, Result};
