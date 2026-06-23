//! Encoding/decoding and content-addressing utilities for Airframe.
//!
//! `airframe_codec` provides a small, object-unsafe [`Codec`] trait with
//! ready-to-use implementations, a content [`Envelope`], content IDs, and
//! base-N encoding helpers.
//!
//! # Key pieces
//! - [`Codec`] — encode/decode trait; see [`codecs`] for built-in implementations.
//! - [`Envelope`] — a `{codec, payload}` wrapper tagging encoded bytes.
//! - [`ContentId`] / [`content_id_sha256`] — content addressing by SHA-256.
//! - [`basexx`] — base16/32/64 helpers.
//! - [`AirframeCodecError`] — the crate error type.
//!
//! # Example
//! ```ignore
//! use airframe_codec::{content_id_sha256, ContentId};
//!
//! let id: ContentId = content_id_sha256(b"some bytes");
//! ```
pub mod basexx;
pub mod codecs;
pub mod content_id;
pub mod envelope;
pub mod error;
#[cfg(feature = "module")]
pub mod module;

pub use content_id::{content_id_sha256, ContentId};
pub use envelope::Envelope;
pub use error::AirframeCodecError;

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Core abstraction over a codec implementation.
pub trait Codec {
    const NAME: &'static str;
    fn encode<T: Serialize>(&self, t: &T) -> Result<Vec<u8>, AirframeCodecError>;
    fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T, AirframeCodecError>;
    fn content_id(&self, bytes: &[u8]) -> ContentId {
        content_id_sha256(bytes)
    }
}
