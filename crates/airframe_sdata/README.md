# airframe_sdata

Short description: Schema-aware typed data helpers on top of airframe_data, with optional protected-at-rest integration via airframe_pdata.

## Overview

airframe_sdata focuses on ergonomics around typed repositories and caches that:
- Embed schema name and version with each record
- Validate values on write/read
- Migrate older stored versions to the current model via a registry of migrators
- Optionally compose with airframe_pdata to protect bytes-at-rest while preserving transform order

## Logical pieces

- model::DataModel: your typed record with SCHEMA_NAME, SCHEMA_VERSION, and optional validate().
- schema::{SchemaRegistry, Migrator}: register and run migrations between schema versions.
- store::{TypedRepo, KeySpace}: typed repository over any airframe_data backend + codec.
- cache::SchemaCache: typed cache over any airframe_data ByteCache + codec.
- protected::* (feature "integration-pdata"; legacy alias: "protected"): ProtectedTypedRepo and ProtectedSchemaCache over airframe_pdata::bytes::PStoreBytes.
- module::{SDataModule, SDataFactory, ServiceRegistrySDataExt}: Airframe module and factory for composing common stacks.

## Airframe module compatibility

- Compatibility: Yes — provides `cap:sdata` via SDataModule
- Optional requires: `cap:pdata` (when using protected helpers)

## Dependencies

- Rust dependencies: see Cargo.toml (airframe_data, serde, anyhow)
- Optional feature `protected`: depends on airframe_pdata and airframe_crypt (via pdata)
- System libraries: none
- Airframe capacities/modules: Exports `cap:sdata` (module), uses `cap:pdata` if available

## Setup / Installation

As a library:
```toml
[dependencies]
airframe_data = { path = "../airframe_data" }
airframe_sdata = { path = "../airframe_sdata" }
```

With protected helpers:
```toml
[dependencies]
airframe_pdata = { path = "../airframe_pdata" }
airframe_sdata = { path = "../airframe_sdata", features = ["protected"] }
```

As an Airframe module providing cap:sdata:
```toml
[dependencies]
airframe_core = { path = "../airframe_core" }
airframe_sdata = { path = "../airframe_sdata" }
```

## Usage

### Example 1: Typed, in-memory repo with a migration
```rust
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use airframe_data::{backend::mem::MemBackend, codec::JsonCodec};
use airframe_sdata::{model::DataModel, schema::{SchemaRegistry, Migrator}, store::{TypedRepo, KeySpace}};
use serde_json::json;

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
struct UserV2 { id: String, name: String, age: u32 }
impl DataModel for UserV2 { const SCHEMA_NAME: &'static str = "user"; const SCHEMA_VERSION: u32 = 2; }

struct UserMigV1toV2;
impl Migrator for UserMigV1toV2 {
    fn schema_name(&self) -> &'static str { "user" }
    fn migrate(&self, _from: u32, _to: u32, mut v: serde_json::Value) -> airframe_sdata::error::Result<serde_json::Value> {
        if let Some(obj) = v.as_object_mut() { obj.entry("age").or_insert(json!(0)); }
        Ok(v)
    }
}

fn main() -> anyhow::Result<()> {
    let mut reg = SchemaRegistry::new();
    reg.register_step("user", 1, Arc::new(UserMigV1toV2));
    let reg = Arc::new(reg);

    let backend = MemBackend::new();
    let codec = JsonCodec::default();
    let repo: TypedRepo<_, _, UserV2> = TypedRepo::new(backend, codec, reg.clone());

    let ks = KeySpace::new("users");
    let key = ks.key("alice").unwrap();
    let v = UserV2 { id: "alice".into(), name: "Alice".into(), age: 1 };
    repo.put(&key, &v)?;
    let got = repo.get(&key)?.unwrap();
    assert_eq!(got, v);
    Ok(())
}
```

### Example 2: Use SDataModule and SDataFactory (cap:sdata)
```rust
use std::sync::Arc;
use airframe_core::app::AppBuilder;
use airframe_sdata::module::{SDataModule, SDataFactory};
use airframe_sdata::{schema::SchemaRegistry, model::DataModel};
use serde::{Serialize, Deserialize};

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
struct UserV1 { id: String, name: String }
impl DataModel for UserV1 { const SCHEMA_NAME: &'static str = "user"; const SCHEMA_VERSION: u32 = 1; }

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new().with(SDataModule::new()).start().await?;
    let factory = app.services.get::<SDataFactory>().expect("factory");

    let mut reg = SchemaRegistry::new();
    let reg = Arc::new(reg);
    let repo = factory.typed_json_mem::<UserV1>(reg);
    // use repo...
    Ok(())
}
```

## Examples
- examples/typed_mem.rs — typed, in-memory unprotected repo
- examples/protected_mem.rs — protected typed repo over in-memory cache (requires `--features airframe_sdata/integration-pdata`; legacy alias: `airframe_sdata/protected`)

Run:
```bash
cargo run -p airframe_sdata --example typed_mem
cargo run -p airframe_sdata --example protected_mem --features airframe_sdata/integration-pdata
# (legacy alias also works: --features airframe_sdata/protected)
```

## Status

Airframe module interface implemented (final step).

## License

This project is licensed under the repository license; see the top-level LICENSE file.
