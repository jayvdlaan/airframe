# airframe_secrets

Short description: Encrypted secrets helpers and an encrypted cache built on airframe_crypt and airframe_data.

## Overview

airframe_secrets focuses on just-in-time decryption and safe handling of sensitive values. It provides wrappers that only expose plaintext inside a caller-provided closure, plus an encrypted cache that stores ciphertext-at-rest over any airframe_data::cache::ByteCache backend.

## Logical pieces

- SecretBlob: encrypted bytes (EnvelopeBytes) with closure-based access to plaintext.
- SecretValue<T>: typed variant over an encrypted envelope with closure-based access to &T.
- SecretCache<BC>: encrypt/decrypt helpers over any ByteCache backend (mem/fs/etc.).
- KeyResolver: optional trait to resolve a key from a key-id; avoids passing raw keys around.
- SecretBytes facade: redacted Debug/Display to minimize accidental leaks (secrecy-backed).

## Airframe module compatibility

- Capability: provides cap:secrets; requires cap:crypt
- Optional requires: cap:health (optional readiness integration if present)
- Module: SecretsModule registers an in-memory SecretCache by default and performs a small encrypt/decrypt health probe at init.
- Config keys (section: `secrets`):
  - `secrets.cipher` = "aes-gcm" | "chacha20" (default: "aes-gcm")
  - `secrets.key.bytes_hex` = "..." (optional; hex-encoded 32-byte key; DEV/TEST only)
  - `secrets.cache.backend` = "mem" (default; placeholders for future backends)
- Example wiring:
  ```rust
  use airframe_core::app::AppBuilder;
  use airframe_secrets::SecretsModule;
  
  let app = AppBuilder::new()
      .with(airframe_crypt::CryptModule::new())
      .with(SecretsModule::new())
      .start().await?;
  // Retrieve the cache
  let cache = app.services.secrets_cache().unwrap();
  ```

## Dependencies

- Rust dependencies: see Cargo.toml (airframe_crypt, airframe_data, secrecy, serde)
- System libraries: none
- Airframe capacities/modules: none

## Setup / Installation

```toml
[dependencies]
airframe_secrets = { path = "../airframe_secrets" }
airframe_crypt = { path = "../airframe_crypt" }
airframe_data = { path = "../airframe_data" }
secrecy = { version = "0.10", features = ["serde"] }
```

## Features

- backend-secrecy: Use the secrecy crate as the SecretBytes backend (default off).
- backend-secrets: Use the secrets crate as the SecretBytes backend (default off; pulls the optional dependency).
- logging: Enables optional integration with airframe_logging (no hard requirement; tracing is used regardless).
- config: Enables reading secrets-related config via airframe_config. Without this, sane defaults are used.
- args: Enables optional CLI argument helpers via airframe_args (for downstream tooling/examples).
- health: Enables readiness/liveness integration with airframe_health; the module registers a small encrypt/decrypt check when HealthModule is present.
- default = []: No backend is enabled by default. SecretBytes will still compile and zeroize a plain buffer as a fallback.
- full meta-feature: enables common features: `features = ["full"]`.
- Legacy aliases: `secret-backend-secrecy` and `secret-backend-secrets` remain as temporary aliases and will be removed in a future major release.

## Usage

### Example 1: SecretBlob — decrypt on demand within a closure

```rust
use airframe_crypt::envelope::EnvelopeBytes;
use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_secrets::SecretBlob;
use secrecy::SecretSlice;

let suite = SoftwareCipherSuite::new();
let key = SecretSlice::new(vec![5u8; 32].into_boxed_slice());
let aad = Some(b"ctx".as_ref());
let pt = SecretSlice::new(Vec::from("secret").into_boxed_slice());
let env = EnvelopeBytes::encrypt_with_suite(&suite, SymmetricAlgorithm::AesGcm, &key, &pt, aad)?;
let blob = SecretBlob::new(env, None);
let echoed = blob.with_plaintext(&suite, &key, aad, |p| p.to_vec())?;
assert_eq!(echoed, b"secret");
```

### Example 2: SecretCache over an in-memory backend

```rust
use airframe_secrets::SecretCache;
use airframe_data::backend::mem::MemBackend;
use airframe_data::cache::BackendByteCache;
use airframe_data::key::Key;
use airframe_crypt::suite::SoftwareCipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_secrets::SecretBytes;

let backend = MemBackend::new();
let bytes = BackendByteCache::new(backend);
let cache = SecretCache::new(bytes);
let suite = SoftwareCipherSuite::new();
let key = SecretBytes::from_vec(vec![7u8; 32]);

#[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq, Debug)]
struct Demo { a: u32, b: String }

let k = Key::new("demo:1")?;
let v = Demo { a: 42, b: "life".into() };
cache.put_value(&k, &suite, SymmetricAlgorithm::AesGcm, &key, &v, None)?;
let out: Demo = cache.get_value(&k, &suite, &key, None)?.unwrap();
assert_eq!(out, v);
```

### Example 3: Load secret with fallback (cache → resolver → default)

See runnable example: `examples/secret_fallback.rs`

Run:

```
cargo run -q -p airframe_secrets --example secret_fallback
```

## Notes and safety

- Plaintext is only held transiently and wrapped in secrecy types; avoid cloning/copying where not necessary.
- All APIs accept optional AAD for AEAD binding.
- Errors are intentionally redacted to avoid leaking crypto internals.

## Maintenance commands

- Coverage (HTML):
  - `cargo llvm-cov -p airframe_secrets --html --output-path target/coverage/airframe_secrets-html`
- Docs:
  - `cargo doc -p airframe_secrets --all-features --no-deps`

## License

Licensed under the MIT License.
