use std::fmt;

/// Errors that can occur during bitwise read operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    /// Not enough bits remaining in the buffer.
    BufferUnderflow { needed: usize, available: usize },
    /// Data could not be decoded (e.g. invalid UTF-8, varint overflow).
    DecodeError(String),
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WireError::BufferUnderflow { needed, available } => {
                write!(
                    f,
                    "buffer underflow: need {needed} bits but only {available} available"
                )
            }
            WireError::DecodeError(msg) => write!(f, "decode error: {msg}"),
        }
    }
}

impl std::error::Error for WireError {}
