# airframe_redis

Short description: Redis-backed ByteCache adapter for Airframe’s data layer.

## Overview

airframe_redis implements the `airframe_data::cache::ByteCache` trait using Redis as the backing store. It supports per-namespace key scoping, optional default TTL via SETEX, and a simple builder to configure the connection URL, namespace, and TTL.

## Logical pieces

- RedisByteCacheBuilder: configure URL, namespace, and default TTL
- RedisByteCache: implements ByteCache over Redis connections
- Namespacing: logical keys mapped to `namespace::key` to avoid collisions
- Listing: `ByteCache::list()` implemented using SCAN with a MATCH prefix

## Features

This crate avoids pulling the heavy `redis` dependency unless requested.

```toml
[features]
default = []                 # no heavy deps by default
driver = ["dep:redis"]       # enable the real Redis driver implementation
module = ["driver"]          # Airframe module integration (implies `driver`)
```

Build tips:
- No features: types are available and compile, but operations are stubbed and return InvalidState.
- `--features driver`: enables the real Redis adapter backed by the `redis` crate.
- `--features module`: enables the Airframe runtime module; this implies `driver` and links `airframe_core`, `airframe_config`, and `airframe_health`.

## Airframe module compatibility

- Capability: provides `cap:cache.redis`
- Feature flag: enable with `features = ["module"]`
- Optional requires: cap:health (optional readiness integration if HealthModule is present)
- Registered services:
  - `Arc<RedisByteCache>` (concrete)
  - `Arc<dyn airframe_data::cache::ByteCache>` (trait-object for decoupled consumers)
- Config keys (via airframe_config BasicConfig, env overrides supported):
  - `redis.url` (default: `redis://127.0.0.1/`; can be overridden by `REDIS_URL` env)
  - `redis.namespace` (default: `app`)
  - `redis.default_ttl_sec` (optional; `0` or unset disables TTL)
- Health/readiness:
  - Fail-fast during init: Redis `PING` + tiny write/read/delete probe.
  - Optional airframe_health integration: if `cap:health` is present (HealthModule wired), registers a required `redis` health check that runs `PING` on demand.

### Example: wiring into AppBuilder (requires `--features module`)

```rust
use airframe_core::app::AppBuilder;
use airframe_redis::RedisModule; // requires feature = "module"

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new()
        // Optionally include ConfigModule first to load configs
        // .with(airframe_config::ConfigModule::new(Some("./Config.toml".into())))
        .with(RedisModule::new())
        .start()
        .await?;

    // Retrieve the cache via extension
    #[cfg(feature = "module")]
    {
        use airframe_redis::ServiceRegistryRedisExt;
        let cache = app.services.redis_byte_cache().expect("Redis cache registered");
        // ... use cache
    }

    Ok(())
}
```

## Dependencies

- Rust dependencies: see Cargo.toml (redis, airframe_data)
- System libraries: none (requires a running Redis server for examples/tests)
- Airframe capacities/modules: none

## Setup / Installation

```toml
[dependencies]
airframe_data = { path = "../airframe_data" }
airframe_redis = { path = "../airframe_redis" }
```

## Usage

### Example 1: Basic put/get with TTL

```rust
use std::time::Duration;
use airframe_data::cache::ByteCache;
use airframe_data::key::Key;
use airframe_redis::RedisByteCacheBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Requires local Redis at 127.0.0.1:6379
    let bytes = RedisByteCacheBuilder::new("redis://127.0.0.1/")
        .namespace("demo")
        .default_ttl(Duration::from_secs(60))
        .build()?;

    let k = Key::new("greeting")?;
    bytes.put_bytes(&k, b"hello")?;
    let out = bytes.get_bytes(&k)?.unwrap();
    assert_eq!(out, b"hello");
    Ok(())
}
```

### Example 2: Without a default TTL

```rust
use airframe_data::cache::ByteCache;
use airframe_data::key::Key;
use airframe_redis::RedisByteCacheBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = RedisByteCacheBuilder::new("redis://127.0.0.1/")
        .namespace("demo")
        .build()?; // no default TTL

    let k = Key::new("counter")?;
    bytes.put_bytes(&k, &123u64.to_be_bytes())?;
    let out = bytes.get_bytes(&k)?.unwrap();
    assert_eq!(out.len(), 8);
    Ok(())
}
```

## Connection URLs and authentication

The `redis` crate URL format is supported, including password and DB index, e.g.:
- `redis://127.0.0.1/0`
- `redis://:password@127.0.0.1:6379/1`

Pass the URL to the builder: `RedisByteCacheBuilder::new(url)`.

## Notes

- `ByteCache::list()` uses SCAN with a MATCH restricted to the namespace prefix and returns logical keys without the namespace prefix.
- This adapter opens a new connection for each operation via `client.get_connection()`. High-throughput deployments may prefer a pooled strategy in a future enhancement.

## Status

Airframe module interface implemented (final step).

## License

This project is licensed under the repository license; see the top-level LICENSE file.
