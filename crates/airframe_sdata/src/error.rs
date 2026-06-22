use airframe_core::error::{AirframeError, ErrorRange};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AirframeSdataError {
    #[error("Success")]
    Success,
    #[error("Core error: {0}")]
    CoreError(#[from] AirframeError),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("Migration error: {0}")]
    MigrationError(String),
    #[error("Codec error: {0}")]
    CodecError(String),
    #[error("Not found")]
    NotFound,
    #[error("Invalid state")]
    InvalidState,
    #[error("Unknown error code: {0}")]
    Other(u32),
}

impl AirframeSdataError {
    pub fn to_int(&self) -> u32 {
        let base = ErrorRange::Sdata.base();
        match self {
            AirframeSdataError::Success => base,
            AirframeSdataError::CoreError(err) => err.to_int(),
            AirframeSdataError::ValidationError(_) => base + 1,
            AirframeSdataError::MigrationError(_) => base + 2,
            AirframeSdataError::CodecError(_) => base + 3,
            AirframeSdataError::NotFound => base + 4,
            AirframeSdataError::InvalidState => base + 5,
            AirframeSdataError::Other(code) => *code,
        }
    }

    pub fn from_int(val: u32) -> Option<Self> {
        if ErrorRange::Core.contains(val) {
            return AirframeError::from_int(val).map(AirframeSdataError::CoreError);
        }
        if ErrorRange::Sdata.contains(val) {
            let code = val - ErrorRange::Sdata.base();
            match code {
                0 => Some(AirframeSdataError::Success),
                1 => Some(AirframeSdataError::ValidationError("unknown".into())),
                2 => Some(AirframeSdataError::MigrationError("unknown".into())),
                3 => Some(AirframeSdataError::CodecError("unknown".into())),
                4 => Some(AirframeSdataError::NotFound),
                5 => Some(AirframeSdataError::InvalidState),
                _ => Some(AirframeSdataError::Other(val)),
            }
        } else {
            Some(AirframeSdataError::Other(val))
        }
    }
}

pub type Result<T> = std::result::Result<T, AirframeSdataError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_int_basic_variants() {
        let base = ErrorRange::Sdata.base();
        assert_eq!(AirframeSdataError::Success.to_int(), base);
        assert_eq!(
            AirframeSdataError::ValidationError("x".into()).to_int(),
            base + 1
        );
        assert_eq!(
            AirframeSdataError::MigrationError("x".into()).to_int(),
            base + 2
        );
        assert_eq!(
            AirframeSdataError::CodecError("x".into()).to_int(),
            base + 3
        );
        assert_eq!(AirframeSdataError::NotFound.to_int(), base + 4);
        assert_eq!(AirframeSdataError::InvalidState.to_int(), base + 5);
        assert_eq!(AirframeSdataError::Other(12345).to_int(), 12345);
    }

    #[test]
    fn to_int_core_passthrough() {
        let core = AirframeError::InvalidState;
        let core_code = core.to_int();
        assert_eq!(AirframeSdataError::CoreError(core).to_int(), core_code);
    }

    #[test]
    fn from_int_roundtrip_known() {
        // Test a few known codes
        let base = ErrorRange::Sdata.base();
        assert!(matches!(
            AirframeSdataError::from_int(base),
            Some(AirframeSdataError::Success)
        ));
        assert!(matches!(
            AirframeSdataError::from_int(base + 4),
            Some(AirframeSdataError::NotFound)
        ));
        assert!(matches!(
            AirframeSdataError::from_int(base + 5),
            Some(AirframeSdataError::InvalidState)
        ));
    }

    #[test]
    fn from_int_core_range_and_other() {
        // Core code should map to CoreError
        let core_code = AirframeError::InvalidArgument.to_int();
        let e = AirframeSdataError::from_int(core_code).unwrap();
        match e {
            AirframeSdataError::CoreError(inner) => {
                assert!(matches!(inner, AirframeError::InvalidArgument))
            }
            _ => panic!("expected core error"),
        }

        // Unknown outside range becomes Other(value)
        let unknown = ErrorRange::Yk.max() + 10; // arbitrary code outside defined ranges
        assert!(
            matches!(AirframeSdataError::from_int(unknown), Some(AirframeSdataError::Other(v)) if v == unknown)
        );
    }
}
