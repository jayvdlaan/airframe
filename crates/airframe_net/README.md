# airframe_net

Async reliable UDP networking primitives for Airframe.

## Overview

`airframe_net` provides low-level building blocks for datagram networking on top
of an unreliable UDP substrate. It is deliberately small and composable — each
piece can be used independently:

- **Transport** (`UdpTransport`): a thin async wrapper over a tokio `UdpSocket`
  for binding, sending, and receiving raw datagrams, with a conservative
  `MAX_UDP_PAYLOAD` (1200 bytes) sized for the public internet.
- **Connection** (`Connection`, `ConnectionState`): a connection-lifecycle state
  machine (`Disconnected` → `Connecting` → `Connected` → `Disconnecting`) that
  validates transitions and tracks last-sent / last-received timestamps.
- **Fragmentation / reassembly** (`FragmentAssembler`): splits oversized messages
  into header-prefixed fragments and reassembles incoming fragments, tolerating
  out-of-order delivery. Pending reassemblies are bounded (with stale-timeout
  cleanup and oldest-eviction) to resist memory-amplification floods.
- **Reliability layer** (`ReliableChannel`, `ReliableConfig`): sequencing,
  cumulative + bitfield acknowledgements, duplicate suppression, retransmission,
  and per-message timeout detection over the otherwise best-effort transport.
- **Peer statistics** (`PeerStats`): per-peer telemetry — median RTT over a
  sliding window, jitter, packet loss ratio, and send/receive throughput.

Errors are surfaced through a single `NetError` enum (`Io`, `ConnectionRefused`,
`Timeout`, `Reset`).

## Airframe module compatibility

- Compatibility: N/A — this crate provides standalone networking primitives. It
  does not implement the Airframe `Module` trait and exports no capability; it is
  used as a plain library by higher-level crates and services.

## Dependencies

- Airframe crates: none — `airframe_net` has zero internal Airframe dependencies.
- External crates: `tokio` (with the `net` feature) for the async UDP socket.
- System libraries: none.

## Usage

The primitives compose, but each is usable on its own. The example below sends a
datagram between two locally-bound transports:

```rust
use airframe_net::UdpTransport;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), airframe_net::NetError> {
    let sender = UdpTransport::bind("127.0.0.1:0".parse().unwrap()).await?;
    let mut receiver = UdpTransport::bind("127.0.0.1:0".parse().unwrap()).await?;

    let dst: SocketAddr = receiver.local_addr()?;
    sender.send_to(b"hello airframe", dst).await?;

    let (data, from) = receiver.recv_from().await?;
    assert_eq!(&data, b"hello airframe");
    assert_eq!(from, sender.local_addr()?);
    Ok(())
}
```

Layering reliability and fragmentation over the transport (sketch):

```rust
use airframe_net::{FragmentAssembler, ReliableChannel, ReliableConfig, MAX_UDP_PAYLOAD};
use std::time::Duration;

// Sender side: frame for reliability, then fragment to fit the MTU.
let mut channel = ReliableChannel::new(ReliableConfig::default());
let mut assembler = FragmentAssembler::new(Duration::from_secs(5));

let (seq, framed) = channel.send(b"a large payload...".to_vec());
let fragments = assembler.fragment(&framed, MAX_UDP_PAYLOAD);
// ... write each fragment to the wire via UdpTransport::send_to ...

// Receiver side: reassemble, then deliver in-order through the channel.
let mut rx_assembler = FragmentAssembler::new(Duration::from_secs(5));
let mut rx_channel = ReliableChannel::new(ReliableConfig::default());
// for each datagram read from the wire:
//   if let Some(message) = rx_assembler.receive_fragment(&datagram)? {
//       if let Some(payload) = rx_channel.receive(&message) { /* deliver */ }
//   }

// Periodically drive retransmissions and detect timeouts on the sender:
let _retransmit: Vec<Vec<u8>> = channel.get_retransmissions();
let _timed_out: Vec<u16> = channel.get_timeouts();
let _ = seq;
```

`PeerStats` can track per-peer health alongside any of the above:

```rust
use airframe_net::PeerStats;
use std::time::Duration;

let mut stats = PeerStats::new();
stats.record_sent(1024);
stats.update_rtt(Duration::from_millis(42));
println!("rtt={:?} loss={:.2}", stats.rtt(), stats.loss_ratio());
```
