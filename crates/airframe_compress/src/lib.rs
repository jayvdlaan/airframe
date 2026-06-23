//! Minimal compression abstraction with pluggable algorithms for Airframe.
//!
//! `airframe_compress` offers whole-buffer compression/decompression via the
//! [`Compressor`] trait, plus streaming helpers to compress while writing and
//! decompress while reading. It is used across Airframe (for example the cache
//! layers in `airframe_data` and the `airframe_pdata` pipeline).
//!
//! # Key pieces
//! - [`Compressor`] — whole-buffer compress/decompress trait.
//! - [`stream`] — streaming compress-on-write / decompress-on-read helpers.
//! - `Gzip`, `Zstd`, `Brotli`, `Lz4` — algorithm implementations, each behind its
//!   own cargo feature.
//! - [`AirframeCompressError`] — the crate error type.
//!
//! # Example
//! ```ignore
//! use airframe_compress::{Compressor, Zstd};
//!
//! let packed = Zstd::default().compress(b"data")?;
//! let original = Zstd::default().decompress(&packed)?;
//! ```
pub mod error;

pub use error::AirframeCompressError;

pub mod compressor;
pub use compressor::Compressor;

pub mod stream;

// Re-export algorithms at crate root to preserve API
#[cfg(feature = "brotli")]
pub use algorithms::brotli::Brotli;
#[cfg(feature = "gzip")]
pub use algorithms::gzip::Gzip;
#[cfg(feature = "lz4")]
pub use algorithms::lz4::Lz4;
#[cfg(feature = "zstd")]
pub use algorithms::zstd::Zstd;

// private module to gather implementations
mod algorithms {
    #[cfg(feature = "brotli")]
    pub mod brotli;
    #[cfg(feature = "gzip")]
    pub mod gzip;
    #[cfg(feature = "lz4")]
    pub mod lz4;
    #[cfg(feature = "zstd")]
    pub mod zstd;
}
