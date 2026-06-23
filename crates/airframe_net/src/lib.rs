//! Async reliable-UDP networking primitives for Airframe.
//!
//! `airframe_net` provides low-level building blocks for datagram networking: a
//! UDP transport, fragmentation/reassembly, peer RTT tracking, and an optional
//! reliability layer (ordered, acknowledged delivery) over UDP.
//!
//! # Key pieces
//! - [`UdpTransport`] — the datagram transport ([`MAX_UDP_PAYLOAD`] payload cap).
//! - [`Connection`] / [`ConnectionState`] — a peer connection and its state.
//! - [`ReliableChannel`] / [`ReliableConfig`] — ordered, acknowledged delivery.
//! - [`FragmentAssembler`] — reassembles fragmented datagrams (with DoS bounds).
//! - [`PeerStats`] — per-peer RTT / loss statistics.
//! - [`NetError`] — the crate error type.
//!
//! # Example
//! ```ignore
//! use airframe_net::transport::UdpTransport;
//!
//! let transport = UdpTransport::bind("127.0.0.1:0").await?;
//! ```
pub mod connection;
mod error;
pub mod fragment;
pub mod peer;
pub mod reliable;
pub mod transport;

pub use connection::{Connection, ConnectionState};
pub use error::NetError;
pub use fragment::FragmentAssembler;
pub use peer::PeerStats;
pub use reliable::{ReliableChannel, ReliableConfig};
pub use transport::{UdpTransport, MAX_UDP_PAYLOAD};
