#![cfg(feature = "gzip")]

use crate::compressor::Compressor;
use crate::AirframeCompressError;
use std::io::Write;
use tracing::{debug, error, instrument};

#[derive(Clone)]
pub struct Gzip {
    pub(crate) level: u32,
}
impl Default for Gzip {
    fn default() -> Self {
        Self { level: 6 }
    }
}
impl Gzip {
    pub fn new(level: u32) -> Self {
        Self { level }
    }
}
impl Compressor for Gzip {
    fn name(&self) -> &'static str {
        "gzip"
    }
    fn default_extension(&self) -> &'static str {
        "gz"
    }
    fn level(&self) -> Option<i32> {
        Some(self.level as i32)
    }
    #[instrument(level = "trace", skip(self, input))]
    fn compress(&self, input: &[u8]) -> Result<Vec<u8>, AirframeCompressError> {
        debug!(target = "airframe_compress", algo = %self.name(), in_len = input.len(), "compress");
        let mut out = Vec::new();
        let mut enc = flate2::write::GzEncoder::new(&mut out, flate2::Compression::new(self.level));
        enc.write_all(input).map_err(|e| {
            error!(target = "airframe_compress", error = ?e, algo = %self.name(), in_len = input.len(), "compress failed (write)");
            AirframeCompressError::CompressError(format!("gzip: {}", e))
        })?;
        enc.finish().map_err(|e| {
            error!(target = "airframe_compress", error = ?e, algo = %self.name(), in_len = input.len(), "compress failed (finish)");
            AirframeCompressError::CompressError(format!("gzip: {}", e))
        })?;
        Ok(out)
    }
    #[instrument(level = "trace", skip(self, input))]
    fn decompress(&self, input: &[u8]) -> Result<Vec<u8>, AirframeCompressError> {
        debug!(target = "airframe_compress", algo = %self.name(), in_len = input.len(), "decompress");
        let dec = flate2::read::GzDecoder::new(input);
        crate::compressor::read_capped(dec, crate::compressor::MAX_DECOMPRESSED_BYTES).map_err(|e| {
            error!(target = "airframe_compress", error = ?e, algo = %self.name(), in_len = input.len(), "decompress failed");
            e
        })
    }
}
