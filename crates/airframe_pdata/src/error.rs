use airframe_core::error::{AirframeError, ErrorRange};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AirframePdataError {
    #[error("Success")]
    Success,
    #[error("Core error: {0}")]
    CoreError(#[from] AirframeError),
    #[error("Invalid state")]
    InvalidState,
    #[error("Unknown error code: {0}")]
    Other(u32),
}

impl AirframePdataError {
    pub fn to_int(&self) -> u32 {
        let base = ErrorRange::Pdata.base();
        match self {
            AirframePdataError::Success => base,
            AirframePdataError::CoreError(err) => err.to_int(),
            AirframePdataError::InvalidState => base + 1,
            AirframePdataError::Other(code) => *code,
        }
    }

    pub fn from_int(val: u32) -> Option<Self> {
        if ErrorRange::Core.contains(val) {
            return AirframeError::from_int(val).map(AirframePdataError::CoreError);
        }
        if ErrorRange::Pdata.contains(val) {
            let code = val - ErrorRange::Pdata.base();
            match code {
                0 => Some(AirframePdataError::Success),
                1 => Some(AirframePdataError::InvalidState),
                _ => Some(AirframePdataError::Other(val)),
            }
        } else {
            Some(AirframePdataError::Other(val))
        }
    }
}

pub type Result<T> = std::result::Result<T, AirframePdataError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_int_mappings() {
        let base = ErrorRange::Pdata.base();
        assert_eq!(AirframePdataError::Success.to_int(), base);
        assert_eq!(AirframePdataError::InvalidState.to_int(), base + 1);
        assert_eq!(AirframePdataError::Other(4242).to_int(), 4242);
        // Core passthrough
        let core_code = AirframeError::InvalidArgument.to_int();
        assert_eq!(
            AirframePdataError::CoreError(AirframeError::InvalidArgument).to_int(),
            core_code
        );
    }

    #[test]
    fn from_int_variants_and_other() {
        let base = ErrorRange::Pdata.base();
        assert!(matches!(
            AirframePdataError::from_int(base),
            Some(AirframePdataError::Success)
        ));
        assert!(matches!(
            AirframePdataError::from_int(base + 1),
            Some(AirframePdataError::InvalidState)
        ));

        // Core code maps to CoreError
        let core_code = AirframeError::InvalidOperation.to_int();
        match AirframePdataError::from_int(core_code).unwrap() {
            AirframePdataError::CoreError(inner) => {
                assert!(matches!(inner, AirframeError::InvalidOperation))
            }
            _ => panic!("expected core error"),
        }

        // Unknown outside any range becomes Other(val)
        let unknown = ErrorRange::Yk.max() + 1234;
        assert!(
            matches!(AirframePdataError::from_int(unknown), Some(AirframePdataError::Other(v)) if v == unknown)
        );
    }
}
