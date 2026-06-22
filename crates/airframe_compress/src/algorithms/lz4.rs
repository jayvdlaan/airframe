#![cfg(feature = "lz4")]

use crate::compressor::Compressor;
use crate::AirframeCompressError;
use tracing::{debug, error, instrument};

#[derive(Clone, Default)]
pub struct Lz4;
impl Lz4 {
    pub fn new() -> Self {
        Self
    }
}
impl Compressor for Lz4 {
    fn name(&self) -> &'static str {
        "lz4"
    }
    fn default_extension(&self) -> &'static str {
        "lz4"
    }
    #[instrument(level = "trace", skip(self, input))]
    fn compress(&self, input: &[u8]) -> Result<Vec<u8>, AirframeCompressError> {
        debug!(target = "airframe_compress", algo = %self.name(), in_len = input.len(), "compress");
        // Use frame for cross-compat with common tools, enable checksums so corruption is detected
        let mut out = Vec::new();
        {
            use lz4_flex::frame::{BlockMode, BlockSize, FrameEncoder, FrameInfo};
            let mut info = FrameInfo::default();
            info.block_mode = BlockMode::Independent;
            info.block_size = BlockSize::Max1MB;
            info.content_checksum = true;
            info.block_checksums = true;
            let mut enc = FrameEncoder::with_frame_info(info, &mut out);
            std::io::copy(&mut &*input, &mut enc)
                .map_err(|e| {
                    error!(target = "airframe_compress", error = ?e, algo = %self.name(), in_len = input.len(), "compress failed");
                    AirframeCompressError::CompressError(format!("lz4: {}", e))
                })?;
            // Explicitly finish the frame (write the end marker + content checksum).
            // Relying on FrameEncoder's Drop swallows finalize errors and could leave
            // a truncated frame that decompresses to nothing.
            enc.finish().map_err(|e| {
                error!(target = "airframe_compress", error = ?e, algo = %self.name(), in_len = input.len(), "compress failed (finish)");
                AirframeCompressError::CompressError(format!("lz4: {}", e))
            })?;
        }
        Ok(out)
    }
    #[instrument(level = "trace", skip(self, input))]
    fn decompress(&self, input: &[u8]) -> Result<Vec<u8>, AirframeCompressError> {
        debug!(target = "airframe_compress", algo = %self.name(), in_len = input.len(), "decompress");
        let dec = lz4_flex::frame::FrameDecoder::new(input);
        crate::compressor::read_capped(dec, crate::compressor::MAX_DECOMPRESSED_BYTES).map_err(|e| {
            error!(target = "airframe_compress", error = ?e, algo = %self.name(), in_len = input.len(), "decompress failed");
            e
        })
    }
}
