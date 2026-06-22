use crate::AirframeCompressError;

/// Default safety cap on decompressed output (bytes).
///
/// Decompression turns a small input into a potentially huge output; without a
/// bound a crafted "decompression bomb" can exhaust memory. 512 MiB is generous
/// for legitimate payloads while still rejecting pathological expansion.
pub const MAX_DECOMPRESSED_BYTES: usize = 512 * 1024 * 1024;

/// Drain a decompression reader to completion, failing if the output would exceed
/// `max` bytes. This is the shared defense against decompression bombs used by all
/// algorithm backends.
pub fn read_capped<R: std::io::Read>(
    mut reader: R,
    max: usize,
) -> Result<Vec<u8>, AirframeCompressError> {
    // Explicit read loop (mirrors std::io::copy) so behavior is identical across
    // decoder backends, plus a running cap so output can never exceed `max`.
    let mut out = Vec::new();
    let mut buf = [0u8; 16 * 1024];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| AirframeCompressError::DecompressError(e.to_string()))?;
        if n == 0 {
            break;
        }
        if out.len() + n > max {
            return Err(AirframeCompressError::DecompressError(format!(
                "decompressed output exceeds {max}-byte safety limit (possible decompression bomb)"
            )));
        }
        out.extend_from_slice(&buf[..n]);
    }
    Ok(out)
}

/// Whole-buffer compression interface implemented by specific algorithms.
pub trait Compressor: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn default_extension(&self) -> &'static str;
    fn level(&self) -> Option<i32> {
        None
    }

    fn compress(&self, input: &[u8]) -> Result<Vec<u8>, AirframeCompressError>;
    fn decompress(&self, input: &[u8]) -> Result<Vec<u8>, AirframeCompressError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoLevel;
    impl Compressor for NoLevel {
        fn name(&self) -> &'static str {
            "none"
        }
        fn default_extension(&self) -> &'static str {
            "bin"
        }
        fn compress(&self, input: &[u8]) -> Result<Vec<u8>, AirframeCompressError> {
            Ok(input.to_vec())
        }
        fn decompress(&self, input: &[u8]) -> Result<Vec<u8>, AirframeCompressError> {
            Ok(input.to_vec())
        }
    }

    struct WithLevel(i32);
    impl Compressor for WithLevel {
        fn name(&self) -> &'static str {
            "lvl"
        }
        fn default_extension(&self) -> &'static str {
            "lvl"
        }
        fn level(&self) -> Option<i32> {
            Some(self.0)
        }
        fn compress(&self, input: &[u8]) -> Result<Vec<u8>, AirframeCompressError> {
            Ok(input.to_vec())
        }
        fn decompress(&self, input: &[u8]) -> Result<Vec<u8>, AirframeCompressError> {
            Ok(input.to_vec())
        }
    }

    #[test]
    fn default_level_is_none() {
        let c = NoLevel;
        assert_eq!(c.level(), None);
        assert_eq!(c.name(), "none");
        assert_eq!(c.default_extension(), "bin");
    }

    #[test]
    fn custom_level_is_respected() {
        let c = WithLevel(7);
        assert_eq!(c.level(), Some(7));
        assert_eq!(c.name(), "lvl");
        assert_eq!(c.default_extension(), "lvl");
    }
}
