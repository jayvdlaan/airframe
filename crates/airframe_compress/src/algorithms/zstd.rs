#![cfg(feature = "zstd")]

use crate::compressor::Compressor;
use crate::AirframeCompressError;
use tracing::{debug, error, instrument};

#[derive(Clone, Default)]
pub struct Zstd {
    level: i32,
}
impl Zstd {
    pub fn new(level: i32) -> Self {
        Self { level }
    }
}
impl Compressor for Zstd {
    fn name(&self) -> &'static str {
        "zstd"
    }
    fn default_extension(&self) -> &'static str {
        "zst"
    }
    fn level(&self) -> Option<i32> {
        Some(self.level)
    }
    #[instrument(level = "trace", skip(self, input))]
    fn compress(&self, input: &[u8]) -> Result<Vec<u8>, AirframeCompressError> {
        debug!(target = "airframe_compress", algo = %self.name(), in_len = input.len(), "compress");
        zstd::bulk::compress(input, if self.level == 0 { 3 } else { self.level })
            .map_err(|e| {
                error!(target = "airframe_compress", error = ?e, algo = %self.name(), in_len = input.len(), "compress failed");
                AirframeCompressError::CompressError(format!("zstd: {}", e))
            })
    }
    #[instrument(level = "trace", skip(self, input))]
    fn decompress(&self, input: &[u8]) -> Result<Vec<u8>, AirframeCompressError> {
        debug!(target = "airframe_compress", algo = %self.name(), in_len = input.len(), "decompress");
        let dec = zstd::stream::read::Decoder::new(input).map_err(|e| {
            error!(target = "airframe_compress", error = ?e, algo = %self.name(), in_len = input.len(), "decompress failed");
            AirframeCompressError::DecompressError(format!("zstd: {}", e))
        })?;
        crate::compressor::read_capped(dec, crate::compressor::MAX_DECOMPRESSED_BYTES).map_err(|e| {
            error!(target = "airframe_compress", error = ?e, algo = %self.name(), in_len = input.len(), "decompress failed");
            e
        })
    }
}
