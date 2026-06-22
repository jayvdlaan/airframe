use airframe_core::error::{AirframeError, ErrorRange};
use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AirframeDataError {
    #[error("Success")]
    Success,
    #[error("Core error: {0}")]
    CoreError(#[from] AirframeError),
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Codec error: {0}")]
    Codec(String),
    #[error("Not found")]
    NotFound,
    #[error("Invalid state")]
    InvalidState,
    #[error("Invalid key: {0}")]
    KeyInvalid(String),
    #[error("Corrupted data")]
    Corrupted,
    #[error("Unknown error code: {0}")]
    Other(u32),
}

impl AirframeDataError {
    pub fn to_int(&self) -> u32 {
        let base = ErrorRange::Data.base();
        match self {
            AirframeDataError::Success => base,
            AirframeDataError::CoreError(err) => err.to_int(),
            AirframeDataError::Io(_) => base + 1,
            AirframeDataError::Codec(_) => base + 2,
            AirframeDataError::NotFound => base + 3,
            AirframeDataError::InvalidState => base + 4,
            AirframeDataError::KeyInvalid(_) => base + 5,
            AirframeDataError::Corrupted => base + 6,
            AirframeDataError::Other(code) => *code,
        }
    }

    pub fn from_int(val: u32) -> Option<Self> {
        if ErrorRange::Core.contains(val) {
            return AirframeError::from_int(val).map(AirframeDataError::CoreError);
        }
        if ErrorRange::Data.contains(val) {
            let code = val - ErrorRange::Data.base();
            match code {
                0 => Some(AirframeDataError::Success),
                1 => Some(AirframeDataError::Io(io::Error::other("Unknown IO error"))),
                2 => Some(AirframeDataError::Codec("Unknown codec error".into())),
                3 => Some(AirframeDataError::NotFound),
                4 => Some(AirframeDataError::InvalidState),
                5 => Some(AirframeDataError::KeyInvalid("unknown".into())),
                6 => Some(AirframeDataError::Corrupted),
                _ => Some(AirframeDataError::Other(val)),
            }
        } else {
            Some(AirframeDataError::Other(val))
        }
    }
}

pub type Result<T> = std::result::Result<T, AirframeDataError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        assert_eq!(format!("{}", AirframeDataError::Success), "Success");
        assert_eq!(format!("{}", AirframeDataError::NotFound), "Not found");
        assert_eq!(
            format!("{}", AirframeDataError::InvalidState),
            "Invalid state"
        );
        assert_eq!(
            format!("{}", AirframeDataError::Corrupted),
            "Corrupted data"
        );
        assert_eq!(
            format!("{}", AirframeDataError::Codec("test".into())),
            "Codec error: test"
        );
        assert_eq!(
            format!("{}", AirframeDataError::KeyInvalid("bad".into())),
            "Invalid key: bad"
        );
    }

    #[test]
    fn to_int_returns_correct_codes() {
        let base = ErrorRange::Data.base();
        assert_eq!(AirframeDataError::Success.to_int(), base);
        assert_eq!(AirframeDataError::NotFound.to_int(), base + 3);
        assert_eq!(AirframeDataError::InvalidState.to_int(), base + 4);
        assert_eq!(AirframeDataError::Corrupted.to_int(), base + 6);
        assert_eq!(AirframeDataError::Other(9999).to_int(), 9999);
    }

    #[test]
    fn from_int_returns_correct_variants() {
        let base = ErrorRange::Data.base();

        assert!(matches!(
            AirframeDataError::from_int(base),
            Some(AirframeDataError::Success)
        ));
        assert!(matches!(
            AirframeDataError::from_int(base + 3),
            Some(AirframeDataError::NotFound)
        ));
        assert!(matches!(
            AirframeDataError::from_int(base + 4),
            Some(AirframeDataError::InvalidState)
        ));
        assert!(matches!(
            AirframeDataError::from_int(base + 6),
            Some(AirframeDataError::Corrupted)
        ));
    }

    #[test]
    fn from_int_unknown_code_returns_other() {
        let result = AirframeDataError::from_int(99999);
        assert!(matches!(result, Some(AirframeDataError::Other(99999))));
    }

    #[test]
    fn from_int_io_error() {
        let base = ErrorRange::Data.base();
        let result = AirframeDataError::from_int(base + 1);
        assert!(matches!(result, Some(AirframeDataError::Io(_))));
    }

    #[test]
    fn from_int_codec_error() {
        let base = ErrorRange::Data.base();
        let result = AirframeDataError::from_int(base + 2);
        assert!(matches!(result, Some(AirframeDataError::Codec(_))));
    }

    #[test]
    fn from_int_key_invalid() {
        let base = ErrorRange::Data.base();
        let result = AirframeDataError::from_int(base + 5);
        assert!(matches!(result, Some(AirframeDataError::KeyInvalid(_))));
    }

    #[test]
    fn io_error_conversion() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let data_err: AirframeDataError = io_err.into();
        assert!(matches!(data_err, AirframeDataError::Io(_)));
    }

    #[test]
    fn core_error_conversion() {
        let core_err = AirframeError::InvalidArgument;
        let data_err: AirframeDataError = core_err.into();
        assert!(matches!(data_err, AirframeDataError::CoreError(_)));
    }

    #[test]
    fn from_int_core_range() {
        // Core errors are in range 0-99
        let result = AirframeDataError::from_int(1); // InvalidArgument in core
        assert!(matches!(result, Some(AirframeDataError::CoreError(_))));
    }
}
