# airframe_kv

Short description: Shared, observable key-value store for inter-module coordination.

## Overview

airframe_kv provides an in-memory key-value store with TTL expiry, CAS via etags, prefix listing, and prefix watches via async streams. When used as a module, it registers the store into the ServiceRegistry and forwards KvEvents to the app EventBus so other modules can react.

## Logical pieces

- Traits: KvStore (async), KvStoreExt (typed serde helpers), KvEvent
- Types: InMemoryKvStore, Metadata (etag/ttl), PutOptions
- Streams: prefix_watch to observe changes under a key prefix

## Airframe module compatibility

- Compatibility: Yes — provides `cap:kv` via KvModule
- Services: registers InMemoryKvStore and Arc<dyn KvStore> into the ServiceRegistry

## Dependencies

- Rust dependencies: see Cargo.toml
- System libraries: none
- Airframe capacities/modules: Exports `cap:kv` when used as a module; publishes KvEvent to the app EventBus

## Setup / Installation

```toml
[dependencies]
airframe_core = { path = "../airframe_core" }
airframe_kv = { path = "../airframe_kv" }
```

Quick start
```rust
use airframe_core::app::AppBuilder;
use airframe_kv::{KvModule, InMemoryKvStore, PutOptions};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new().with(KvModule::new()).start().await?;
    let kv = app.services.get::<InMemoryKvStore>().expect("kv present");
    kv.put("demo/key", b"value", PutOptions { ttl: Some(Duration::from_secs(5)), if_match: None }).await?;
    Ok(())
}
```

Notes
- Use namespaced keys like `module/sub/key` to avoid collisions.
- Use `PutOptions { if_match: Some(etag) }` for CAS updates.


## Trait-object usage and typed helpers
You can depend on the trait object (Arc<dyn KvStore>) from the ServiceRegistry to remain backend-agnostic. Use KvStoreExt for typed serde helpers:

```rust
use std::time::Duration;
use std::sync::Arc;
use airframe_core::app::AppBuilder;
use airframe_kv::{KvModule, KvStore, KvStoreExt, PutOptions};

#[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq, Debug)]
struct FeatureFlag { enabled: bool }

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new().with(KvModule::new()).start().await?;
    let kv: Arc<dyn KvStore> = app.services.get::<dyn KvStore>().expect("kv present");

    // Typed put/get
    KvStoreExt::put_t(&*kv, "feature/foo", &FeatureFlag { enabled: true }, PutOptions { ttl: Some(Duration::from_secs(60)), if_match: None }).await?;
    let (flag, meta): (FeatureFlag, _) = KvStoreExt::get_t(&*kv, "feature/foo").await?.unwrap();
    assert!(flag.enabled);
    println!("etag={}", meta.etag);
    Ok(())
}
```

## Filesystem backend (feature-gated)

A filesystem-backed implementation of `KvStore` is available behind the `kv-fs` feature. It provides the same core semantics as the in-memory store: get/put/delete with CAS (etag), per-key TTL (persisted), prefix listing + pagination, and prefix watches (best-effort, in-process events). Writes are made durable via temp-file + fsync + atomic rename, and the parent directory is fsynced to persist metadata updates.

Conformance: a shared test suite runs against both InMemoryKvStore and FilesystemKvStore to verify parity for put/get/delete, CAS, TTL, listing, and pagination.

TTL semantics: TTL is enforced on read and via a background janitor. The filesystem backend persists absolute deadlines in file headers, so TTL expiry works across process restarts.

Enable the feature and use it as follows:

```toml
# Cargo.toml
[dependencies]
airframe_kv = { path = "../airframe_kv", features = ["kv-fs"] }
```

```rust
use std::time::Duration;
use airframe_kv::{FilesystemKvStore, KvStore, PutOptions};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Open or create a store under ./data
    let kv = FilesystemKvStore::open("./data").await?;
    kv.put("demo/key", b"value", PutOptions { ttl: Some(Duration::from_secs(5)), if_match: None }).await?;
    Ok(())
}
```

Notes
- Watch events are currently in-process only; external file changes are not observed.
- TTL is enforced on read and via a background janitor task that emits `KvEvent::Expire`.
- Listing and pagination walk the directory tree and sort keys lexicographically; suitable for local/single-node use.

## Status

- InMemoryKvStore ready for dev use.
- FilesystemKvStore available behind `kv-fs` feature for local/single-node durability.
- Airframe module interface implemented.

## License
This project is licensed under the repository license; see the top-level LICENSE file.
