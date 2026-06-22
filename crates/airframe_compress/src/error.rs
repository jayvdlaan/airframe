use airframe_core::error::ErrorRange;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AirframeCompressError {
    #[error("Success")]
    Success,
    #[error("Core error: {0}")]
    CoreError(#[from] airframe_core::error::AirframeError),
    #[error("Compression error: {0}")]
    CompressError(String),
    #[error("Decompression error: {0}")]
    DecompressError(String),
    #[error("Invalid data: {0}")]
    InvalidData(String),
    #[error("Unsupported: {0}")]
    Unsupported(String),
    #[error("Unknown error code: {0}")]
    Other(u32),
}

impl AirframeCompressError {
    /// Map to a stable integer code. Until a dedicated ErrorRange exists for compression,
    /// we temporarily use the Codec range to avoid overlap with Core and others.
    pub fn to_int(&self) -> u32 {
        let base = ErrorRange::Codec.base();
        match self {
            AirframeCompressError::Success => base,
            AirframeCompressError::CoreError(err) => err.to_int(),
            AirframeCompressError::CompressError(_) => base + 1,
            AirframeCompressError::DecompressError(_) => base + 2,
            AirframeCompressError::InvalidData(_) => base + 3,
            AirframeCompressError::Unsupported(_) => base + 4,
            AirframeCompressError::Other(code) => *code,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airframe_core::error::AirframeError;

    #[test]
    fn display_variants() {
        assert_eq!(format!("{}", AirframeCompressError::Success), "Success");
        assert_eq!(
            format!("{}", AirframeCompressError::CompressError("oops".into())),
            "Compression error: oops"
        );
        assert_eq!(
            format!("{}", AirframeCompressError::DecompressError("bad".into())),
            "Decompression error: bad"
        );
        assert_eq!(
            format!("{}", AirframeCompressError::InvalidData("why".into())),
            "Invalid data: why"
        );
        assert_eq!(
            format!("{}", AirframeCompressError::Unsupported("lz99".into())),
            "Unsupported: lz99"
        );
        assert_eq!(
            format!("{}", AirframeCompressError::Other(42)),
            "Unknown error code: 42"
        );
        assert_eq!(
            format!(
                "{}",
                AirframeCompressError::CoreError(AirframeError::InvalidArgument)
            ),
            "Core error: Invalid argument"
        );
    }

    #[test]
    fn codes_are_in_codec_range_or_core() {
        let codec_base = ErrorRange::Codec.base();
        let core_base = ErrorRange::Core.base();
        assert_eq!(AirframeCompressError::Success.to_int(), codec_base);
        assert_eq!(
            AirframeCompressError::CompressError("x".into()).to_int(),
            codec_base + 1
        );
        assert_eq!(
            AirframeCompressError::DecompressError("x".into()).to_int(),
            codec_base + 2
        );
        assert_eq!(
            AirframeCompressError::InvalidData("x".into()).to_int(),
            codec_base + 3
        );
        assert_eq!(
            AirframeCompressError::Unsupported("x".into()).to_int(),
            codec_base + 4
        );
        assert_eq!(
            AirframeCompressError::CoreError(AirframeError::Success).to_int(),
            core_base
        );
    }
}
