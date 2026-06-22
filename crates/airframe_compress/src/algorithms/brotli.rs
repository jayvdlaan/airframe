#![cfg(feature = "brotli")]

use crate::compressor::Compressor;
use crate::AirframeCompressError;
use std::io::Write;
use tracing::{debug, error, instrument};

#[derive(Clone)]
pub struct Brotli {
    pub(crate) quality: u32,
}
impl Default for Brotli {
    fn default() -> Self {
        Self { quality: 5 }
    }
}
impl Brotli {
    pub fn new(quality: u32) -> Self {
        Self { quality }
    }
}
impl Compressor for Brotli {
    fn name(&self) -> &'static str {
        "brotli"
    }
    fn default_extension(&self) -> &'static str {
        "br"
    }
    fn level(&self) -> Option<i32> {
        Some(self.quality as i32)
    }
    #[instrument(level = "trace", skip(self, input))]
    fn compress(&self, input: &[u8]) -> Result<Vec<u8>, AirframeCompressError> {
        debug!(target = "airframe_compress", algo = %self.name(), in_len = input.len(), "compress");
        let mut out = Vec::new();
        {
            let mut enc = brotli::CompressorWriter::new(&mut out, 4096, self.quality, 22);
            enc.write_all(input).map_err(|e| {
                error!(target = "airframe_compress", error = ?e, algo = %self.name(), in_len = input.len(), "compress failed (write)");
                AirframeCompressError::CompressError(format!("brotli: {}", e))
            })?;
            // drop(enc) will flush
        }
        Ok(out)
    }
    #[instrument(level = "trace", skip(self, input))]
    fn decompress(&self, input: &[u8]) -> Result<Vec<u8>, AirframeCompressError> {
        debug!(target = "airframe_compress", algo = %self.name(), in_len = input.len(), "decompress");
        let dec = brotli::Decompressor::new(input, 4096);
        crate::compressor::read_capped(dec, crate::compressor::MAX_DECOMPRESSED_BYTES).map_err(|e| {
            error!(target = "airframe_compress", error = ?e, algo = %self.name(), in_len = input.len(), "decompress failed");
            e
        })
    }
}
