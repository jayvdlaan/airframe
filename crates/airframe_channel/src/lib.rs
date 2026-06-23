//! A Noise_XX mutually-authenticated, encrypted byte-stream channel for Airframe.
//!
//! `airframe_channel` implements the Noise Protocol Framework `XX` handshake
//! with the `Noise_XX_25519_ChaChaPoly_SHA256` suite — X25519 key agreement,
//! ChaCha20-Poly1305 AEAD, and SHA-256 / HKDF. All cryptographic primitives are
//! delegated to `airframe_crypt`; this crate contributes the Noise state
//! machine, message framing, and transport glue. After the three-message `XX`
//! handshake each side has authenticated the other's long-term static public key.
//!
//! # Key pieces
//! - [`noise`] — the Noise `XX` state machine ([`HandshakeState`], [`TransportState`]).
//! - [`framing`] — length-delimited message framing.
//! - [`ChannelError`] — the crate error type.
//! - `Channel` / `NoiseSession`, plus the `TcpChannel` (feature `tcp`) and
//!   `UdsChannel` (feature `uds`) transports — an established channel over a stream.
//!
//! # Example
//! ```ignore
//! // With feature `tcp`: establish a mutually-authenticated channel, then use it.
//! let channel = airframe_channel::tcp_initiator(stream, &my_static_key).await?;
//! channel.send(b"hello").await?;
//! let reply = channel.recv().await?;
//! ```
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
