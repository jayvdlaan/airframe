use airframe_core::error::AirframeError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AirframePgError {
    #[error("Success")]
    Success,
    #[error("Core error: {0}")]
    CoreError(#[from] AirframeError),
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Query error: {0}")]
    Query(String),
    #[error("Migration error: {0}")]
    Migration(String),
    #[error("Invalid state")]
    InvalidState,
    #[error("Unknown error code: {0}")]
    Other(u32),
}

pub type Result<T> = std::result::Result<T, AirframePgError>;
