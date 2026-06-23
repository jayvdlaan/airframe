# airframe_pdata

Short description: Protected-at-rest data layer (CtE pipeline + context) built on airframe_data with optional compression and module integration.

## Overview

airframe_pdata provides a small, opinionated layer for protecting data at rest using AEAD encryption with correct transform ordering. It composes with airframe_data backends/caches and offers both bytes-level and typed repositories. Optional plaintext compression can be applied inside the protection pipeline.

Critical invariant (ordering):
- Write: serialize (Codec) → optional compress → encrypt (AEAD) → persist (ByteCache/KvBackend)
- Read: load → decrypt → optional decompress → deserialize

This ensures we never “compress ciphertext,” a common layering pitfall.

## Logical pieces

- context::PContext<R: KeyResolver>: holds cipher suite, algorithm, AAD policy, key resolver + optional key_id, optional namespace, compression policy.
- bytes::PStoreBytes<BC, R>: bytes-level protected store over any airframe_data::cache::ByteCache stack. Enforces CtE/DtD.
- typed::PStore<C, BC, R>: typed protected store bridging an airframe_data::codec::Codec over PStoreBytes.
- builder::PDataBuilder: ergonomic builder to assemble bytes/typed stores.
- policy::{Compression, AadPolicy,...}: opt-in compression and AAD composition utilities.
- module::{PDataModule, ServiceRegistryPDataExt, PDataFactory}: Airframe module and helpers to obtain prebuilt contexts and in-memory stores.

## Airframe module compatibility

- Compatibility: Yes — provides `cap:pdata` via PDataModule
- Services: registers PDataFactory into the ServiceRegistry to construct common pdata stacks

## Dependencies

- Rust dependencies/features: see Cargo.toml
  - airframe_data (backends/caches/codecs)
  - airframe_crypt (cipher suite, AEAD algorithms)
  - airframe_secrets (key material wrappers and resolvers)
  - Feature `compress` (optional): enables airframe_compress inside the CtE pipeline
- System libraries: none (pure Rust; crypt uses software by default)
- Airframe capacities/modules: Exports `cap:pdata` when used as a module; may require `cap:crypt`

## Setup / Installation

Library-only:
```toml
[dependencies]
airframe_data = { path = "../airframe_data" }
airframe_pdata = { path = "../airframe_pdata" }
airframe_crypt = { path = "../airframe_crypt" }
airframe_secrets = { path = "../airframe_secrets" }
```

With optional compression:
```toml
[dependencies]
airframe_compress = { path = "../airframe_compress" }
airframe_pdata = { path = "../airframe_pdata", features = ["compress"] }
```

As an Airframe module:
```toml
[dependencies]
airframe_core = { path = "../airframe_core" }
airframe_crypt = { path = "../airframe_crypt" }
airframe_pdata = { path = "../airframe_pdata" }
```

## Usage

### Example 1: Typed, in-memory protected store
```rust
// See detailed example below in this README under "Quick start (typed, in-memory)"
```

### Example 2: Bytes-level API over any ByteCache
```rust
// See detailed example below in this README under "Bytes-level API"
```

## License

Licensed under the MIT License.

## Crate features
- `compress` (optional): enable plaintext compression via airframe_compress::Compressor inside the CtE pipeline. Disabled by default for lean builds.

## Concepts
- PContext<R: KeyResolver>: holds the cipher suite, algorithm, AAD policy, key resolver + optional key_id, optional namespace, and compression policy.
- PStoreBytes<BC, R>: bytes-level protected store over any airframe_data::cache::ByteCache stack. Enforces CtE/DtD.
- PStore<C, BC, R>: typed protected store bridging an airframe_data::codec::Codec over PStoreBytes.
- PDataBuilder: ergonomic builder to assemble bytes/typed stores.

## Quick start (typed, in-memory)
```rust
use airframe_data::{backend::mem::MemBackend, cache::BackendByteCache, codec::JsonCodec, key::Key};
use airframe_crypt::{suite::SoftwareCipherSuite, sym::SymmetricAlgorithm};
use airframe_pdata::{builder::PDataBuilder, context::{KeyResolver, PContext}};
use airframe_pdata::typed::PStore;
use airframe_secrets::secret::SecretBytes;

#[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq, Debug)]
struct User { id: u32, name: String }

struct StaticResolver { key: SecretBytes }
impl KeyResolver for StaticResolver {
    fn resolve(&self, _key_id: Option<&[u8]>) -> Result<SecretBytes, airframe_pdata::AirframePdataError> {
        Ok(SecretBytes::from_vec(self.key.to_vec()))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = BackendByteCache::new(MemBackend::new());
    let resolver = StaticResolver { key: SecretBytes::from_vec(vec![7u8; 32]) };
    let suite = SoftwareCipherSuite::new();
    let mut ctx = PContext::new(suite, SymmetricAlgorithm::ChaCha20Poly1305, resolver);
    ctx.namespace = Some("users".into());

    let store: PStore<_, _, _> = PDataBuilder::new()
        .bytes(bytes)
        .context(ctx)
        .build_typed(JsonCodec::default())?;

    let k = Key::new("u:1")?;
    let v = User { id: 1, name: "Luna".into() };
    store.put(&k, &v)?;
    let got: User = store.get(&k)?.unwrap();
    assert_eq!(got, v);
    Ok(())
}
```

## Bytes-level API
```rust
use airframe_data::{backend::mem::MemBackend, cache::BackendByteCache, key::Key};
use airframe_crypt::{suite::SoftwareCipherSuite, sym::SymmetricAlgorithm};
use airframe_pdata::{bytes::PStoreBytes, context::{KeyResolver, PContext}};
use airframe_secrets::secret::SecretBytes;

struct StaticResolver { key: SecretBytes }
impl KeyResolver for StaticResolver {
    fn resolve(&self, _key_id: Option<&[u8]>) -> Result<SecretBytes, airframe_pdata::AirframePdataError> {
        Ok(SecretBytes::from_vec(self.key.to_vec()))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = BackendByteCache::new(MemBackend::new());
    let resolver = StaticResolver { key: SecretBytes::from_vec(vec![5u8; 32]) };
    let ctx = PContext::new(SoftwareCipherSuite::new(), SymmetricAlgorithm::AesGcm, resolver);

    let store = PStoreBytes::new(bytes, ctx);
    let k = Key::new("blob:1")?;
    let data = b"hello pdata".to_vec();
    store.put_bytes(&k, &data)?;
    assert_eq!(store.get_bytes(&k)?.unwrap(), data);
    Ok(())
}
```

## Layering with airframe_data
PStoreBytes sits on top of any ByteCache stack you compose in airframe_data (e.g., LRU front + FS back with read-through, TTL, Namespaces). These layers operate on ciphertext JSON and are safe to use below pdata. Compression must remain inside pdata’s pipeline to preserve CtE order.

## Examples
- examples/typed_mem.rs — typed, in-memory protected store
- examples/module_mem.rs — as an Airframe module (cap:pdata), in-memory demo

Run:
```bash
cargo run -p airframe_pdata --example typed_mem
cargo run -p airframe_pdata --example module_mem
```

(Additional examples like compress_cte and two-level read-through can be added; enable `--features airframe_pdata/compress` for compression.)

## As an Airframe module
Wire AppBuilder with CryptModule and PDataModule (optionally SecretsModule) and obtain PDataFactory from the ServiceRegistry:

```rust
use std::sync::Arc;
use airframe_core::app::AppBuilder;
use airframe_data::key::Key;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_pdata::module::{PDataModule, ServiceRegistryPDataExt};

struct StaticResolver;
impl airframe_secrets::KeyResolver for StaticResolver {
    fn resolve(&self, _key_id: Option<&[u8]>) -> airframe_secrets::error::Result<airframe_secrets::SecretBytes> {
        Ok(airframe_secrets::SecretBytes::from_vec(vec![9u8; 32]))
    }
}

#[tokio::main]
async fn main() {
    let app = AppBuilder::new()
        .with(airframe_crypt::CryptModule::new())
        .with(airframe_pdata::module::PDataModule::new())
        .start().await.unwrap();

    let pd = app.services.pdata_factory().expect("PDataFactory present");
    let ctx = pd.context_with_secrets(SymmetricAlgorithm::ChaCha20Poly1305, Arc::new(StaticResolver));
    let bytes = pd.bytes_mem(ctx.clone());
    let k = Key::new("users:1").unwrap();
    bytes.put_bytes(&k, b"hello").unwrap();
}
```

## Security notes
- Plaintext exists only transiently; encryption uses secrecy-backed SecretSlice via SecretBytes::with_secrecy_slice.
- AAD (namespace + logical key) prevents cross-context replay; extend AadPolicy to include schema/type IDs as needed.

## License
MIT
