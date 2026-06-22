use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChannelError {
    #[error("cryptographic operation failed: {0}")]
    Crypto(#[from] airframe_crypt::error::AirframeCryptError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("OpenSSL error: {0}")]
    OpenSsl(#[from] openssl::error::ErrorStack),

    #[error("framing error: {0}")]
    Framing(String),

    #[error("handshake failed: {0}")]
    Handshake(String),

    #[error("message too large: {size} bytes (max {max})")]
    MessageTooLarge { size: usize, max: usize },

    #[error("nonce exhausted")]
    NonceExhausted,

    #[error("decryption failed")]
    DecryptionFailed,

    #[error("unexpected end of stream")]
    UnexpectedEof,
}
