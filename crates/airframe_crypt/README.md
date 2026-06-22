# airframe_crypt

## Overview

Cryptographic operations for the Airframe SDK, built on OpenSSL. Provides a comprehensive set of primitives for secure data handling: symmetric/asymmetric crypto, hashing, KDF, OTP, RNG, and signatures.

## Logical pieces

- suite::CipherSuite: high-level entry point aggregating crypto operations
- sym: symmetric encryption (e.g., AES-GCM)
- asym: asymmetric crypto (keygen, sign/verify, encrypt/decrypt)
- hash: digest algorithms (SHA-2, etc.)
- kdf: key derivation (PBKDF2, HKDF, ...)
- otp: one-time password utilities
- rng: secure random byte generation

## Airframe module compatibility

- Compatibility: Yes — provides `cap:crypt` via `CryptModule`
- Service access: `ServiceRegistryCryptExt::crypt()` returns the `CipherSuite`

## Dependencies

- Rust dependencies: see Cargo.toml
- System libraries: OpenSSL (headers and runtime) must be available for your platform
  - Linux: libssl-dev, libcrypto
  - macOS: provided by system or via Homebrew (openssl@3)
  - Windows: use vcpkg or prebuilt OpenSSL
- Airframe capacities/modules: Exposes the `cap:crypt` capability when used as a module.

## Setup / Installation

```toml
[dependencies]
airframe_crypt = { path = "../airframe_crypt" }
```

Ensure OpenSSL is installed on your system and linkable by Rust (via pkg-config or environment variables as appropriate for your platform).

## Usage

### Example 1: As an Airframe module
Register the crypt capability and retrieve the suite via the ServiceRegistry:

```rust
use airframe_core::app::AppBuilder;
use airframe_crypt::{CryptModule, ServiceRegistryCryptExt};
use airframe_crypt::suite::CipherSuite;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new()
        .with(CryptModule::new())
        .start()
        .await?;

    let suite = app.services.crypt().expect("crypt suite");

    // Digest
    let hash = suite.digest(airframe_crypt::hash::DigestAlgorithm::Sha256, b"hello")?;
    println!("sha256 bytes = {}", hash.len());

    // AEAD
    use airframe_crypt::sym::SymmetricAlgorithm;
    let key = suite.random_bytes(32)?;
    let nonce = suite.random_bytes(12)?;
    let ct = suite.sym_encrypt(SymmetricAlgorithm::AesGcm, &key, &nonce, b"secret", None)?;
    let pt = suite.sym_decrypt(SymmetricAlgorithm::AesGcm, &key, &nonce, &ct, None)?;
    assert_eq!(&pt, b"secret");

    Ok(())
}
```

### Example 2: Derive a key and encrypt/decrypt

This small example derives a 32-byte key from a password using PBKDF2-HMAC-SHA256 and then encrypts/decrypts a message using AES-256-GCM. It also demonstrates using Zeroizing to wipe the derived key when it goes out of scope.

Run:

```
cargo run -q -p airframe_crypt --example derive_encrypt
```

See: examples/derive_encrypt.rs

### Example 3: Direct crate usage (asymmetric keys and signatures)

```rust
use airframe_crypt::suite::CipherSuite;
use airframe_crypt::asym::{KeyAlgorithm, SignatureAlgorithm};

fn main() -> anyhow::Result<()> {
    let suite = CipherSuite::default();

    // Generate a keypair and sign
    let (pk, sk) = suite.keypair(KeyAlgorithm::Ed25519)?;
    let msg = b"important message";
    let sig = suite.sign(SignatureAlgorithm::Ed25519, &sk, msg)?;
    suite.verify(SignatureAlgorithm::Ed25519, &pk, msg, &sig)?;
    println!("signature verified");

    Ok(())
}
```

## Argon2 (feature-gated)

Argon2 is available behind the optional `argon2` cargo feature. Prefer Argon2id for user-supplied secrets (per RFC 9106) because it combines resistance to GPU/ASIC attacks with side‑channel protections.

Enable the feature and use the `CipherSuite::argon2` API or run the example:

```toml
[dependencies]
airframe_crypt = { path = "../airframe_crypt", features = ["argon2"] }
```

Example:

```bash
cargo run -p airframe_crypt --features argon2 --example kdf_argon2
```

Recommended parameters
- Variant: Argon2id (default)
- Interactive login profile (RFC 9106): memory = 64–128 MiB (m_cost_kib = 65_536..131_072), time = 2–3 (t_cost), parallelism = 1–4 (p_cost)
- Version: 0x13 (v1.3)

Notes
- Minimum salt length is enforced (>= 16 bytes); generate with `suite.random_bytes` and store alongside metadata.
- Output length is capped to 64 bytes (intended for KDF keying, not bulk expansion).
- Defaults follow RFC 9106 "interactive" profile: Argon2id, m=64 MiB, t=3, p=1, v=0x13.

## Zeroization

Where secrets are handled directly (e.g., derived keys), prefer using secrecy::Secret or zeroize::Zeroizing wrappers to help ensure memory is cleared on drop. The derive_encrypt example uses Zeroizing<Vec<u8>> for the derived key. Many higher-level APIs return opaque key types to minimize exposure.

## Integration guidance (Nanokey)

In the Nanokey workspace, enable Argon2 in airframe_crypt and prefer Argon2id for user factors. Use per-factor random salts (>=16 bytes) stored in SecretCache. Combine salts and a stable context to form the final KDF salt.

Cargo.toml (workspace member using Nanokey):

```toml
airframe_crypt = { path = "../airframe/crates/airframe_crypt", features = ["argon2"] }
```

## Status

Airframe module interface implemented (final step).

## License

This project is licensed under the repository license; see the top-level LICENSE file.