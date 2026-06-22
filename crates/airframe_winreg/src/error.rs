use thiserror::Error;
use airframe_core::error::{AirframeError, ErrorRange};

#[derive(Debug, Error)]
pub enum AirframeWinregError {
    #[error("Success")] 
    Success,
    #[error("Core error: {0}")]
    CoreError(#[from] AirframeError),
    #[error("Invalid state")]
    InvalidState,
    #[error("Unknown error code: {0}")]
    Other(u32),
}

impl AirframeWinregError {
    pub fn to_int(&self) -> u32 {
        let base = ErrorRange::Winreg.base();
        match self {
            AirframeWinregError::Success => base + 0,
            AirframeWinregError::CoreError(err) => err.to_int(),
            AirframeWinregError::InvalidState => base + 1,
            AirframeWinregError::Other(code) => *code,
        }
    }

    pub fn from_int(val: u32) -> Option<Self> {
        if ErrorRange::Core.contains(val) {
            return AirframeError::from_int(val).map(AirframeWinregError::CoreError);
        }
        if ErrorRange::Winreg.contains(val) {
            let code = val - ErrorRange::Winreg.base();
            match code {
                0 => Some(AirframeWinregError::Success),
                1 => Some(AirframeWinregError::InvalidState),
                _ => Some(AirframeWinregError::Other(val)),
            }
        } else {
            Some(AirframeWinregError::Other(val))
        }
    }
}

pub type Result<T> = std::result::Result<T, AirframeWinregError>;
