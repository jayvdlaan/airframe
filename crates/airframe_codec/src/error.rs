use airframe_core::error::ErrorRange;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AirframeCodecError {
    #[error("Core error: {0}")]
    CoreError(String),
    #[error("Encode error: {0}")]
    EncodeError(String),
    #[error("Decode error: {0}")]
    DecodeError(String),
    #[error("Invalid data: {0}")]
    InvalidData(String),
    #[error("Unsupported: {0}")]
    Unsupported(String),
}

impl AirframeCodecError {
    pub fn to_int(&self) -> u32 {
        let base = ErrorRange::Codec.base();
        match self {
            AirframeCodecError::CoreError(_) => base,
            AirframeCodecError::EncodeError(_) => base + 1,
            AirframeCodecError::DecodeError(_) => base + 2,
            AirframeCodecError::InvalidData(_) => base + 3,
            AirframeCodecError::Unsupported(_) => base + 4,
        }
    }
}
