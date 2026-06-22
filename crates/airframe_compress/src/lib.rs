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
