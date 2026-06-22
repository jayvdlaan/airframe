# airframe_codec

Encoding/decoding utilities and serialization-format integration for Airframe.

## Overview

`airframe_codec` provides a small object-unsafe `Codec` trait plus three ready-to-use
implementations — `JsonCodec`, `CborCodec`, and `BincodeCodec` — for serializing and
deserializing any `serde` type. Alongside the codecs it offers:

- `codecs::der` — ASN.1 DER encode/decode helpers for `rasn` types.
- `basexx` — base16/base32/base64 encode/decode helpers and minimal multibase prefixing.
- `content_id` — SHA-256 content addressing (`ContentId`, `content_id_sha256`), backed by
  `airframe_crypt`'s OpenSSL digest.
- `Envelope` — a self-describing wrapper that tags an encoded payload with the codec name
  used to produce it, so the right codec can be asserted on unpack.
- `AirframeCodecError` — the crate's error type, with a stable `to_int()` mapping into the
  Airframe error-range scheme.

The `Codec` trait is intentionally not object-safe: its `encode`/`decode` methods are
generic over the `serde` type. For dynamic, name-based selection use the
`module::CodecRegistry` / `module::CodecImpl` enum (see below).

## Airframe module compatibility

This crate works as a standalone library. With the optional `module` feature it also
exports an Airframe runtime module, `airframe_codec::module::CodecModule`.

`CodecModule` provides the capability `cap:codec` and registers two services into the
`ServiceRegistry`:

- `CodecRegistry` — a name → `CodecImpl` map, pre-populated with `cbor`, `json`, and
  `bincode`.
- `DefaultCodec` — the process-wide default codec (CBOR unless overridden).

`CodecImpl` is a `Copy` enum (`Cbor`, `Json`, `Bincode`) that exposes `name()`, `encode()`,
and `decode()`, providing dynamic codec selection without trait objects.

With the additional `config` feature, `CodecModule` optionally reads `cap:config`
(`airframe_config::api::types::BasicConfig`) during init to select the default codec.
Unknown names fall back to CBOR with a warning.

```toml
[codec]
default = "cbor" # or "json" | "bincode"
```

## Dependencies

- Airframe crates: `airframe_core`, `airframe_crypt`; optional `airframe_macros` and
  `airframe_config` (feature-gated).
- Serialization: `serde`, `serde_bytes`, `serde_json`, `serde_cbor`, `bincode`, `rasn`
  (ASN.1 DER).
- Encoding/hashing helpers: `base64`, `data-encoding`.
- Misc: `thiserror`, `tracing`.
- Feature `module` additionally pulls in `toml`, `async-trait`, `semver`, `tokio-util`,
  `anyhow`, and `airframe_macros`.
- System libraries: OpenSSL, via `airframe_crypt` (used for SHA-256 content IDs).

## Usage

```rust
use airframe_codec::codecs::{CborCodec, JsonCodec};
use airframe_codec::{content_id_sha256, Codec, ContentId, Envelope};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Greeting {
    who: String,
    n: u32,
}

fn main() -> Result<(), airframe_codec::AirframeCodecError> {
    let value = Greeting {
        who: "Alice".into(),
        n: 7,
    };

    // Direct codec round-trip.
    let json = JsonCodec;
    let bytes = json.encode(&value)?;
    let back: Greeting = json.decode(&bytes)?;
    assert_eq!(back, value);

    // Envelope tags the payload with the codec name and asserts it on unpack.
    let cbor = CborCodec;
    let env = Envelope::pack(&cbor, &value)?;
    assert_eq!(env.codec, "cbor");
    let unpacked: Greeting = env.unpack(&cbor)?;
    assert_eq!(unpacked, value);

    // Content addressing (SHA-256) and base16 rendering.
    let cid: ContentId = content_id_sha256(&env.payload);
    println!("payload cid = {}", cid.to_hex());

    Ok(())
}
```

Additional runnable examples live under `examples/`: `cbor_roundtrip.rs`,
`envelope_bincode.rs`, `content_id.rs`, and `der_encode.rs`.

## Status

Pre-release, version 0.5.0-beta. API may change before the 1.0 release.

Licensed under MIT.
