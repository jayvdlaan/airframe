pub mod error;
pub mod framing;
pub mod hkdf;
pub mod noise;

#[cfg(any(feature = "tcp", feature = "uds"))]
pub mod channel;

#[cfg(feature = "tcp")]
pub mod tcp;

#[cfg(all(feature = "uds", target_family = "unix"))]
pub mod uds;

// Re-exports
pub use error::ChannelError;
pub use noise::{handshake_xx, HandshakeState, TransportState};

#[cfg(any(feature = "tcp", feature = "uds"))]
pub use channel::{handshake_initiator, handshake_responder, Channel, NoiseSession};

#[cfg(feature = "tcp")]
pub use tcp::{tcp_initiator, tcp_responder, TcpChannel};

#[cfg(all(feature = "uds", target_family = "unix"))]
pub use uds::{uds_initiator, uds_responder, UdsChannel};
