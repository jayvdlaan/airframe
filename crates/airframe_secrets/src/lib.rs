//! Encrypted secrets handling and an encrypted-at-rest cache for Airframe,
//! built on [`airframe_crypt`] and [`airframe_data`].
//!
//! The crate is built around just-in-time decryption: secret wrappers keep
//! ciphertext at rest and only expose plaintext *inside a caller-provided
//! closure*, minimizing how long sensitive bytes live in memory. An encrypted
//! cache stores ciphertext over any [`airframe_data`] `ByteCache` backend.
//!
//! # Key pieces
//! - [`SecretBlob`] — encrypted bytes with closure-scoped access to the plaintext.
//! - [`SecretValue`] — typed variant: closure-scoped access to a decrypted `&T`.
//! - [`SecretBytes`] — owned secret bytes with careful handling.
//! - [`SecretCache`] — encrypt/decrypt over any `ByteCache` backend (mem, fs, …).
//! - [`KeyResolver`] — resolve a key from a key id, so raw keys aren't passed around.
//! - [`SecretsModule`] — Airframe module exposing the secrets service (`cap:secrets`).
//!
//! # Example
//! ```ignore
//! use airframe_secrets::SecretBlob;
//!
//! // Plaintext is only visible inside the closure; it isn't returned.
//! blob.with_plaintext(|bytes| {
//!     // use `bytes` here
//! })?;
//! ```
pub mod error;
pub mod factors;
pub mod module;
pub mod resolver;
pub mod secret;
pub mod secret_blob;
pub mod secret_cache;
pub mod secret_value;

pub use module::{SecretsModule, ServiceRegistrySecretsExt};
pub use resolver::KeyResolver;
pub use secret::SecretBytes;
pub use secret_blob::SecretBlob;
pub use secret_cache::SecretCache;
pub use secret_value::SecretValue;
