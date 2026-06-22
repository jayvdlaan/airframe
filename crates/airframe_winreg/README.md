# airframe_winreg

Short description: Windows Registry adapter providing a ByteCache implementation for airframe_data.

## Overview

airframe_winreg implements the airframe_data::cache::ByteCache trait using the Windows Registry. Values are stored as REG_BINARY under a configurable root path in either HKEY_CURRENT_USER (HKCU) or HKEY_LOCAL_MACHINE (HKLM). The adapter is synchronous and intentionally minimal.

## Features

Optional integration features are available. All are off by default.

```toml
[features]
default = []
logging = ["airframe_logging"]   # enable logging integration (no direct use required)
config  = ["airframe_config"]    # enable configuration loading in WinRegModule
args    = ["airframe_args"]      # reserved for future CLI integration

[dependencies]
airframe_logging = { version = "*", optional = true }
airframe_config  = { version = "*", optional = true }
airframe_args    = { version = "*", optional = true }
```

Build matrix tips:
- Non-Windows targets compile stubs that return InvalidState. Tests are skipped appropriately.
- The module-based example/tests that read configuration require the `config` feature.

## Logical pieces

- WinRegByteCache: ByteCache implementation backed by Windows Registry values.
- HiveKind: enum selecting the registry hive (CurrentUser or LocalMachine).
- Keys and value mapping: uses airframe_data::key::Key for validated logical keys; each key maps to a REG_BINARY value under the configured root path.

## Airframe module compatibility

- Compatibility: Yes — provides `cap:cache.winreg` via `WinRegModule` (Windows-only)
- Optional requires: cap:health (optional readiness integration if present)
- Health/readiness:
  - Fail-fast during init via write/read/delete probe on a temp key.
  - Optional airframe_health integration: if `cap:health` is present (HealthModule wired), registers a required `winreg` health check that performs a tiny write/read/delete.
- Config keys (via airframe_config BasicConfig):
  - `winreg.hive` = `HKCU` | `HKLM` (default: `HKCU`)
  - `winreg.path` = registry path, e.g., `Software\\Airframe\\Cache` (default)
- Health/readiness:
  - Fail-fast during init: tiny write/read/delete probe under the configured path.
  - Optional airframe_health integration: if `cap:health` is present (HealthModule wired), registers a required `winreg` health check that uses the same probe.
- Example wiring:

```rust
use airframe_core::app::AppBuilder;
use airframe_config::ConfigModule;
#[cfg(target_os = "windows")]
use airframe_winreg::WinRegModule;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let defaults = toml::toml! {
        [winreg]
        hive = "HKCU"
        path = "Software\\Airframe\\DemoCache"
    };
    let app = AppBuilder::new()
        .with(ConfigModule::new(None).with_defaults(defaults))
        .with(WinRegModule::new())
        .start().await?;
    // WinRegByteCache is now registered as both concrete and `dyn ByteCache`
    Ok(())
}
```

## Dependencies

- Rust dependencies: see Cargo.toml (`winreg` on Windows targets; `airframe_data` for traits)
- System libraries: Windows Registry (Windows-only). On non-Windows targets, a stub compiles but returns InvalidState.
- Airframe capacities/modules: none

## Setup / Installation

```toml
[dependencies]
airframe_data = { path = "../airframe_data" }
airframe_winreg = { path = "../airframe_winreg" }
```

## Usage

### Example 1: Basic put/get/remove (Windows)

```rust
use airframe_data::cache::ByteCache;
use airframe_data::key::Key;
use airframe_winreg::{WinRegByteCache, HiveKind};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Root path where values will be stored
    let cache = WinRegByteCache::new(HiveKind::CurrentUser, r"Software\\Airframe\\DemoCache");

    let k = Key::new("hello")?;
    cache.put_bytes(&k, b"world")?;
    assert!(cache.contains(&k)?);

    let out = cache.get_bytes(&k)?.unwrap();
    assert_eq!(out, b"world");

    cache.remove(&k)?;
    Ok(())
}
```

### Example 2: List entries under a prefix

```rust
use airframe_data::cache::ByteCache;
use airframe_data::key::Key;
use airframe_winreg::{WinRegByteCache, HiveKind};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cache = WinRegByteCache::new(HiveKind::CurrentUser, r"Software\\Airframe\\DemoCache");
    let _ = cache.put_bytes(&Key::new("a")?, b"1");
    let _ = cache.put_bytes(&Key::new("b")?, b"2");
    // List logical keys (names) stored under the configured root path
    for k in cache.list()? {
        println!("key={}", k);
    }
    Ok(())
}
```

## How values are stored

- Registry Hive: HKCU or HKLM
- Root path: e.g., `Software\\Airframe\\DemoCache`
- For each Key, the value name is the key’s string (validated by airframe_data::Key)
- Value type: REG_BINARY containing the raw bytes

## Cross-platform notes

- Non-Windows targets: constructing WinRegByteCache succeeds but methods return `AirframeDataError::InvalidState`. This allows cross-platform builds while only enabling functionality on Windows.

## Commands

- Coverage (crate only):
  - `cargo llvm-cov -p airframe_winreg --html --output-path target/coverage/airframe_winreg-html`
- Docs:
  - `cargo doc -p airframe_winreg --all-features --no-deps`

## Status

APIs implemented (Windows Registry byte cache adapter).

## License

This project is licensed under the repository license; see the top-level LICENSE file.
