use std::fmt;

/// Network errors for the airframe_net transport layer.
#[derive(Debug)]
pub enum NetError {
    /// The underlying I/O operation failed.
    Io(std::io::Error),
    /// The connection was refused or a state transition was invalid.
    ConnectionRefused(String),
    /// The connection timed out.
    Timeout,
    /// The connection was reset by the peer.
    Reset,
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::ConnectionRefused(msg) => write!(f, "connection refused: {msg}"),
            Self::Timeout => write!(f, "connection timeout"),
            Self::Reset => write!(f, "connection reset"),
        }
    }
}

impl std::error::Error for NetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for NetError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
