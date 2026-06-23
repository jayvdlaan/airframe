# airframe_channel

A Noise_XX secure channel for the Airframe SDK, providing mutually authenticated, encrypted byte streams over TCP and Unix domain sockets.

## Overview

`airframe_channel` implements the [Noise Protocol Framework](https://noiseprotocol.org/) `XX` handshake pattern with the `Noise_XX_25519_ChaChaPoly_SHA256` cipher suite:

- X25519 for Diffie-Hellman key agreement
- ChaCha20-Poly1305 for AEAD encryption
- SHA-256 for hashing and HKDF

All cryptographic primitives are delegated to `airframe_crypt` (built on OpenSSL); this crate contributes the Noise state machine, framing, and transport glue. The `XX` pattern provides mutual authentication: after the three-message handshake each side learns the other's long-term static public key.

### Logical pieces

- `noise::HandshakeState` — the Noise_XX handshake state machine (`write_message_1/2/3`, `read_message_1/2/3`, `finalize`)
- `noise::TransportState` — post-handshake bidirectional cipher (`send` / `recv`)
- `noise::handshake_xx` — drives a full in-memory handshake between two parties (no I/O)
- `hkdf` — RFC 5869 HKDF-SHA256 (`hkdf_extract`, `hkdf_expand`, `hkdf_sha256`) used by the Noise key schedule
- `framing` — length-prefixed (4-byte LE) frames with a 65535-byte max message size, plus async read/write helpers
- `channel::Channel` / `channel::NoiseSession` — an async `send`/`recv` trait and a session wrapping read/write halves with a `TransportState` (requires the `tcp` or `uds` feature)
- `tcp` / `uds` — convenience `*_initiator` / `*_responder` handshake helpers over `TcpStream` / `UnixStream`

### Features

- `tcp` — async TCP channel helpers (pulls in `tokio` + `async-trait`)
- `uds` — async Unix-domain-socket channel helpers (Unix targets only)
- `module` — reserved for Airframe module integration (pulls in `airframe_core`, `airframe_macros`, `semver`); see below
- `full` — enables `tcp`, `uds`, and `module`
- `default` — none

## Airframe module compatibility

- Compatibility: No — this crate does not currently expose an Airframe `Module` or any capability. There is no `ModuleDescriptor` implementation in the source.
- The `module` feature flag is declared and gates optional dependencies (`airframe_core`, `airframe_macros`, `semver`), but no module type is implemented yet. Use the library API directly.

## Dependencies

- Internal: `airframe_crypt` (X25519, ChaCha20-Poly1305, SHA-256, HKDF/HMAC); `airframe_core`, `airframe_macros` (optional, behind the `module` feature)
- External: `openssl` (system OpenSSL headers and runtime required), `thiserror`; `tokio` and `async-trait` (optional, behind `tcp` / `uds`); `semver` (optional, behind `module`)

System OpenSSL must be installed and linkable for your platform (e.g. `libssl-dev` on Linux, `openssl@3` via Homebrew on macOS, vcpkg/prebuilt on Windows).

## Usage

Add the crate with the transport feature you need:

```toml
[dependencies]
airframe_channel = { path = "../airframe_channel", features = ["tcp"] }
airframe_crypt = { path = "../airframe_crypt" }
```

### Encrypted TCP channel

Each peer supplies its long-term X25519 static keypair (a `PKey<Private>`). The
initiator and responder perform the `XX` handshake, then exchange encrypted
messages via the `Channel` trait.

```rust
use airframe_channel::{tcp_initiator, tcp_responder, channel::Channel};
use airframe_crypt::asym::openssl_x25519_generate;
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let server_key = openssl_x25519_generate()?;
    let client_key = openssl_x25519_generate()?;

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut session = tcp_responder(stream, server_key).await.unwrap();
        let msg = session.recv().await.unwrap();
        assert_eq!(msg, b"hello server");
        session.send(b"hello client").await.unwrap();
    });

    let stream = TcpStream::connect(addr).await?;
    let mut session = tcp_initiator(stream, client_key).await?;
    session.send(b"hello server").await?;
    let reply = session.recv().await?;
    assert_eq!(reply, b"hello client");

    server.await?;
    Ok(())
}
```

For Unix domain sockets, enable the `uds` feature and use `uds_initiator` /
`uds_responder` with a `tokio::net::UnixStream` instead.

### In-memory handshake (no I/O)

`handshake_xx` runs the full three-message handshake between two parties and
returns their `TransportState` cipher pair directly, which is useful for tests
or non-socket transports:

```rust
use airframe_channel::handshake_xx;
use airframe_crypt::asym::openssl_x25519_generate;

let initiator_static = openssl_x25519_generate().unwrap();
let responder_static = openssl_x25519_generate().unwrap();

let (mut initiator, mut responder) = handshake_xx(initiator_static, responder_static).unwrap();

let ciphertext = initiator.send(b"hello from initiator").unwrap();
let plaintext = responder.recv(&ciphertext).unwrap();
assert_eq!(plaintext, b"hello from initiator");
```

To inspect the peer's authenticated static key or the channel-binding handshake
hash, drive `HandshakeState` manually and call `finalize`, which returns
`(TransportState, Option<remote_static_pubkey>, handshake_hash)`.
