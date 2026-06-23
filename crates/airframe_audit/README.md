# airframe_audit

Tamper-evident, hash-chained audit logging for Airframe.

## Overview

`airframe_audit` provides an append-only audit log whose entries are linked
into a SHA-256 hash chain. Each `AuditEntry` records a monotonic sequence
number, a timestamp, the hash of the previous entry (`prev_hash`), and the hash
of its own canonical form (`entry_hash`). Because every entry commits to the one
before it, any modification, reordering, or deletion of a past entry breaks the
chain and is detected on verification.

Entries can optionally carry an Ed25519 signature over their `entry_hash`. The
`AuditChain` coordinator supports three signing policies, controlled by
`AuditChainConfig`:

- **Sign every entry** — `require_signing: true` and `sign_interval: 0` (the
  default). Each appended entry is signed; verification then treats a missing
  signature as tampering (signature stripping).
- **Batch signing** — `sign_interval: N > 0`. Every Nth entry is signed,
  amortizing signing cost across the batch.
- **No auto-signing** — `require_signing: false` and `sign_interval: 0`. Entries
  are hash-chained only; you may still force a signature on the chain tip with
  `sign_tip()`.

Cryptography and storage are pluggable behind the `AuditCrypto` and `AuditStore`
traits. Verification (`verify`, `verify_range`) recomputes each entry's
canonical hash, checks the chain links and sequence numbers, and validates any
present signatures, reporting the first broken link via `VerifyResult`.

The canonical byte form hashed for each entry is:

```
seq (8-byte BE) || timestamp (8-byte BE) || prev_hash (UTF-8) || JSON(event)
```

where `JSON(event)` is deterministic JSON with recursively sorted keys, so the
same event always produces the same hash.

## Airframe module compatibility

Yes. `AuditModule` implements the Airframe `Module` trait and exposes a
`ModuleDescriptor` (name `airframe_audit`) that provides the `cap:audit`
capability. During `init`, if both an `AuditCrypto` and an `AuditStore` are
already registered in the `ServiceRegistry`, the module constructs an
`AuditChain` (using a registered `AuditChainConfig` if present, otherwise the
default) and registers it as a service. If those dependencies are absent, the
module is a no-op and expects manual wiring.

The `ServiceRegistryAuditExt` trait adds an `audit_chain()` accessor to
`ServiceRegistry` for retrieving the registered `Arc<AuditChain>`.

## Dependencies

Internal (Airframe):

- `airframe_core` — `Module`, `ModuleContext`, `ModuleDescriptor`,
  `ServiceRegistry`, and the `ErrorRange` integration for `AuditError`.
- `airframe_macros` — `module_descriptor!` macro used by `AuditModule`.
- `airframe_crypt` *(optional, enabled by the `software` feature; also a
  dev-dependency)* — provides the OpenSSL-backed `CipherSuite` used by
  `SoftwareAuditCrypto`.

Notable external:

- `async-trait` — async trait methods on `AuditCrypto`, `AuditStore`, `Module`.
- `tokio` — `Mutex`/`RwLock` synchronization (`sync`, `time` features).
- `serde` / `serde_json` — entry/event serialization and canonical JSON.
- `base64` — signature encoding.
- `thiserror` — `AuditError`.
- `tracing` — diagnostics.

## Usage

The `software` feature provides `SoftwareAuditCrypto`, a local OpenSSL-backed
crypto backend suitable for tests and single-process use. Paired with the
built-in `InMemoryAuditStore`, you can append and verify a chain:

```rust
use std::sync::Arc;

use airframe_audit::{
    AuditChain, AuditChainConfig, AuditEvent, InMemoryAuditStore, SoftwareAuditCrypto,
};
use airframe_crypt::suite::{CipherSuite, SoftwareCipherSuite};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Crypto backend (software, OpenSSL-backed) and storage backend.
    let suite: Arc<dyn CipherSuite> = Arc::new(SoftwareCipherSuite::new());
    let crypto = Arc::new(SoftwareAuditCrypto::generate(suite)?);
    let store = Arc::new(InMemoryAuditStore::new());

    // Default policy signs every entry.
    let chain = AuditChain::new(crypto, store, AuditChainConfig::default());

    // Append an audit event.
    let entry = chain
        .append(AuditEvent {
            event_type: "vault.open".into(),
            status: "success".into(),
            actor: "user1".into(),
            target: Some("vault-abc".into()),
            details: None,
        })
        .await?;
    println!("appended seq {} hash {}", entry.seq, entry.entry_hash);

    // Verify the whole chain.
    let result = chain.verify().await?;
    assert!(result.valid);
    println!(
        "checked {} entries, {} signatures verified",
        result.entries_checked, result.signatures_verified
    );

    Ok(())
}
```

> Note: `SoftwareAuditCrypto` is marked `#[deprecated]` because it performs
> cryptography outside the Nanokey boundary. Production deployments should
> implement `AuditCrypto` against Nanokey's Ed25519 endpoints (e.g.
> `NanokeyAuditCrypto`) rather than using the software backend.
