# airframe_data

Short description: Composable data layer with backends (bytes), typed repos, and layered caches.

## Overview

airframe_data provides small, predictable building blocks for data storage:
- Backends (KvBackend) for storing raw bytes (in-memory and filesystem provided).
- Typed repositories (Repo<B, C>) that serialize/deserialize values using a Codec.
- A layered cache system built around a ByteCache trait, plus a typed Cache<V> bridged by SerdeCache/CodecCache.

The design lets you freely compose layers (LRU, TTL, namespacing, read-through, compression) independent of how data is serialized (Codec) and where bytes are stored (KvBackend).

## Logical pieces

- backend::mem::MemBackend: in-memory bytes backend.
- backend::fs::FsBackend: filesystem-backed bytes with atomic writes and codec-derived file extension.
- key::Key: validated key type used across backends and caches.
- codec::{JsonCodec, …}: simple codecs used in examples; other codecs may live in airframe_codec.
- repo::{Repo, RepoBuilder}: typed read/write API over a backend+codec.
- cache::ByteCache: bytes cache trait designed for layering; adapters and decorators under cache::*.
- cache::Cache<V>, SerdeCache<C, BC>, CodecCache: typed cache shims over ByteCache.

## Airframe module compatibility

- Compatibility: No — this crate is a library; it does not export an Airframe module.

## Dependencies

- Rust dependencies/features: see Cargo.toml
  - Feature `integration-compress` (legacy alias: `compress`): enables the compression cache layer via airframe_compress.
- System libraries: none
- Airframe capacities/modules: none

## Setup / Installation

```toml
[dependencies]
airframe_data = { path = "../airframe_data" }
# Optional algorithms used by examples/features
airframe_compress = { path = "../airframe_compress", optional = true }
```

Enable compression features in your build only when needed:

```bash
cargo run -p airframe_data --example cache_compress --features airframe_data/integration-compress
```

## Usage

### Example 1: Typed repository over Mem and FS

```rust
use airframe_data::{repo::RepoBuilder, backend::{mem::MemBackend, fs::FsBackend}, codec::JsonCodec, key::Key};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Profile { name: String, age: u8 }

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // In-memory
    let repo = RepoBuilder::new()
        .backend(MemBackend::new())
        .codec(JsonCodec::default())
        .build()?;

    let k = Key::new("user:alice")?;
    let v = Profile { name: "Alice".into(), age: 30 };
    repo.put(&k, &v)?;
    let out: Profile = repo.get(&k)?.unwrap();
    assert_eq!(out, v);

    // Filesystem
    let tmp = tempfile::tempdir()?;
    let fs = FsBackend::new(tmp.path(), JsonCodec::default().file_extension())?;
    let repo_fs = RepoBuilder::new().backend(fs).codec(JsonCodec::default()).build()?;
    repo_fs.put(&k, &v)?;
    Ok(())
}
```

### Example 2: Read-through two-level cache with typed view

```rust
use airframe_data::{cache::{BackendByteCache}, backend::{mem::MemBackend, fs::FsBackend}, codec::JsonCodec, key::Key};
use airframe_data::cache::{LruByteCache, ReadThroughByteCache};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Item { id: u32, name: String }

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let front = LruByteCache::new(1024); // hot in-memory
    let tmp = tempfile::tempdir()?;
    let fs = FsBackend::new(tmp.path(), JsonCodec::default().file_extension())?;
    let back = BackendByteCache::new(fs); // bytes view over FS

    // Compose read-through
    let rt = ReadThroughByteCache::new(front, back);

    // Use as a typed cache via SerdeCache
    let typed = airframe_data::cache::SerdeCache::new(JsonCodec::default(), rt);
    let k = Key::new("item:7")?;
    let v = Item { id: 7, name: "seven".into() };
    <_ as airframe_data::cache::Cache<Item>>::put(&typed, &k, &v)?;
    let out: Item = <_ as airframe_data::cache::Cache<Item>>::get(&typed, &k)?.unwrap();
    assert_eq!(out, v);
    Ok(())
}
```

Additional examples live in `examples/` (TTL + Namespace, Compression, Codec shim, FS bytecache, etc.). See the section at the end for commands.

## Examples

Run examples:

```bash
# basic typed cache/backends
cargo run -p airframe_data --example cache_basic
cargo run -p airframe_data --example cache_readthrough
cargo run -p airframe_data --example bytecache_fs

# feature-gated examples
cargo run -p airframe_data --example cache_compress --features airframe_data/integration-compress
cargo run -p airframe_data --example cache_layers_composed --features airframe_data/integration-compress
cargo run -p airframe_data --example cache_codec_shim --features airframe_data/codec-shim
# (legacy aliases also work: compress, codec_shim)
```

## Status

APIs implemented (repos, caches, layers). No Airframe module interface.

## License

This project is licensed under the repository license; see the top-level LICENSE file.
