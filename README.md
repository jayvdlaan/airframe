# Airframe SDK

Airframe is a Rust SDK providing core functionality for secure data handling, cryptographic operations, and storage solutions.

## Overview

Airframe SDK is designed to be a modular and extensible framework for building secure applications. It consists of several crates and runnable examples to help you get started quickly:

- airframe_core: core module system and registry primitives
- airframe_crypt: cryptographic suite(s) and algorithms
- airframe_data: composable storage backends, caches, codecs, and key types
- airframe_secrets: simple secrets interfaces, resolvers, and secret bytes facade
- airframe_pdata: protected-at-rest layer on top of airframe_data (CtE pipeline + context)
- airframe_sdata: schema-aware typed repos and caches with optional protected integration

**Note**: This SDK is in beta (v0.5.0-beta). APIs are stabilizing but may still change before 1.0.

## Key Features

- **Comprehensive Error Handling**: Consistent error types across all crates
- **Cryptographic Operations**: Support for symmetric/asymmetric encryption, hashing, key derivation, and more
- **Storage Solutions**: File-based key-value storage with JSON serialization
- **Modular App Wiring**: Capability-sorted module initialization with shared service registry (see `airframe_core`).

## Installation

Add the crates you need to your `Cargo.toml`:

```toml
[dependencies]
airframe_core = "0.5.0-beta"
airframe_crypt = "0.5.0-beta"
airframe_data = "0.5.0-beta"
```

## Crates

Below is an index of all Airframe crates. Each link points to the crate’s README which follows a consistent layout (Overview, Airframe module compatibility, Dependencies, Setup, Usage, Status).

- [airframe_api](crates/airframe_api/README.md) - Base API contracts, shared types, and common traits
- [airframe_args](crates/airframe_args/README.md) - Command-line arguments and env parsing as an Airframe module
- [airframe_audit](crates/airframe_audit/README.md) - Tamper-evident, hash-chained audit logging
- [airframe_channel](crates/airframe_channel/README.md) - Noise_XX mutually authenticated encrypted channel over TCP and Unix sockets
- [airframe_codec](crates/airframe_codec/README.md) - Content codecs, envelopes, and content IDs
- [airframe_compress](crates/airframe_compress/README.md) - Compression algorithms and streaming helpers
- [airframe_config](crates/airframe_config/README.md) - Config loading/merging and reload events
- [airframe_core](crates/airframe_core/README.md) - Runtime, module system, and service registry
- [airframe_crypt](crates/airframe_crypt/README.md) - Cryptographic primitives, KDFs, and suites
- [airframe_data](crates/airframe_data/README.md) - Storage abstractions: repos, caches, layers
- [airframe_db](crates/airframe_db/README.md) - Database abstractions and adapters
- [airframe_event](crates/airframe_event/README.md) - Event contracts, types, and dispatch patterns
- [airframe_health](crates/airframe_health/README.md) - Health checks (readiness/liveness) integration; HTTP probe routes available when enabling `airframe_health` feature `http`
- [airframe_http](crates/airframe_http/README.md) - HTTP client/server adapters and module
- [airframe_id](crates/airframe_id/README.md) - Shared identifier types for the ceremony framework (InstallId, CeremonyId, Threshold, etc.)
- [airframe_ipc](crates/airframe_ipc/README.md) - IPC primitives: shared memory, Unix sockets, and child processes
- [airframe_kv](crates/airframe_kv/README.md) - Key-value store abstraction and adapters
- [airframe_log_api](crates/airframe_log_api/README.md) - Lightweight logging API: Logger trait, global setter, and no-op macros
- [airframe_logging](crates/airframe_logging/README.md) - Structured logging facade and sinks
- [airframe_macros](crates/airframe_macros/README.md) - Declarative macros for Airframe module development
- [airframe_metrics](crates/airframe_metrics/README.md) - Metrics primitives (Counter, Gauge, DurationMetric) with Prometheus exposition
- [airframe_mysql](crates/airframe_mysql/README.md) - MySQL database adapter (features: driver)
- [airframe_net](crates/airframe_net/README.md) - Async reliable UDP networking primitives
- [airframe_pdata](crates/airframe_pdata/README.md) - Protected-data (CtE) pipeline on top of data
- [airframe_pg](crates/airframe_pg/README.md) - PostgreSQL adapter for Airframe DB abstractions
- [airframe_prefab](crates/airframe_prefab/README.md) - Prefab presets for CLI and Service apps (see docs/PREFABS.md)
- [airframe_recovery_bundle](crates/airframe_recovery_bundle/README.md) - Recovery bundle format and K-of-N share-combining for the ceremony framework
- [airframe_redis](crates/airframe_redis/README.md) - Redis adapter and module with optional health (features: driver, module)
- [airframe_scheduler](crates/airframe_scheduler/README.md) - Job scheduler and timers
- [airframe_sdata](crates/airframe_sdata/README.md) - Schema-aware typed repos and caches
- [airframe_secrets](crates/airframe_secrets/README.md) - Secret resolution interfaces and in-memory cache
- [airframe_sqlite](crates/airframe_sqlite/README.md) - SQLite database adapter (features: driver)
- [airframe_tabular](crates/airframe_tabular/README.md) - Config-driven tabular ingest (CSV/TSV) to typed rows
- [airframe_winreg](crates/airframe_winreg/README.md) - Windows registry-based ByteCache provider (features: logging, config, args)
- [airframe_wire](crates/airframe_wire/README.md) - Bit-level binary protocol primitives

## Crate Status

Legend: Unimplemented | Partially implemented | APIs implemented | Airframe module interface implemented (final step) | TBD (to be categorized)

- [airframe_api](crates/airframe_api/README.md): APIs implemented (base contracts)
- [airframe_args](crates/airframe_args/README.md): Airframe module interface implemented (final step)
- [airframe_codec](crates/airframe_codec/README.md): APIs implemented (codecs, envelope, content IDs)
- [airframe_compress](crates/airframe_compress/README.md): APIs implemented (compression algorithms and streaming helpers)
- [airframe_config](crates/airframe_config/README.md): Airframe module interface implemented (final step)
- [airframe_core](crates/airframe_core/README.md): APIs implemented (runtime and module system)
- [airframe_crypt](crates/airframe_crypt/README.md): Airframe module interface implemented (final step)
- [airframe_data](crates/airframe_data/README.md): APIs implemented (repos, caches, layers)
- [airframe_db](crates/airframe_db/README.md): Airframe module interface implemented (final step)
- [airframe_event](crates/airframe_event/README.md): APIs implemented (event contracts and types)
- [airframe_health](crates/airframe_health/README.md): Airframe module interface implemented (final step)
- [airframe_http](crates/airframe_http/README.md): APIs implemented; module interfaces implemented (behind features)
- [airframe_kv](crates/airframe_kv/README.md): Airframe module interface implemented (final step)
- [airframe_log_api](crates/airframe_log_api/README.md): APIs implemented (lightweight logging facade)
- [airframe_logging](crates/airframe_logging/README.md): Airframe module interface implemented (final step)
- [airframe_mysql](crates/airframe_mysql/README.md): Partially implemented (early adapter)
- [airframe_pdata](crates/airframe_pdata/README.md): Airframe module interface implemented (final step)
- [airframe_prefab](crates/airframe_prefab/README.md): APIs implemented (prefab presets)
- [airframe_redis](crates/airframe_redis/README.md): Airframe module interface implemented (final step; optional cap:health integration for readiness checks)
- [airframe_scheduler](crates/airframe_scheduler/README.md): Airframe module interface implemented (final step)
- [airframe_sdata](crates/airframe_sdata/README.md): Airframe module interface implemented (final step)
- [airframe_secrets](crates/airframe_secrets/README.md): Airframe module interface implemented (initial; cap:secrets; mem cache + health probe)
- [airframe_sqlite](crates/airframe_sqlite/README.md): Partially implemented (minimal synchronous adapter)
- [airframe_winreg](crates/airframe_winreg/README.md): Airframe module interface implemented (Windows-only registry ByteCache; cap:cache.winreg)

## Capability Map

Map of capability strings (cap:*) to provider crates and notes. Feature-gated capabilities are noted.

- cap:args → [airframe_args](crates/airframe_args/README.md)
- cap:crypt → [airframe_crypt](crates/airframe_crypt/README.md)
- cap:kv → [airframe_kv](crates/airframe_kv/README.md)
- cap:scheduler → [airframe_scheduler](crates/airframe_scheduler/README.md)
  - cap:health → [airframe_health](crates/airframe_health/README.md )
- cap:sdata → [airframe_sdata](crates/airframe_sdata/README.md)
- cap:pdata → [airframe_pdata](crates/airframe_pdata/README.md)
- cap:http.server → [airframe_http](crates/airframe_http/README.md) (features: server + module)
- cap:http.client → [airframe_http](crates/airframe_http/README.md) (features: client + module)
- cap:logging → [airframe_logging](crates/airframe_logging/README.md)

Notes:
- Some crates integrate with capabilities without exporting one (e.g., airframe_config registers BasicConfig and emits ConfigReloaded but does not define a cap name).
- Modules that perform internal readiness probes (e.g., db, redis, winreg, secrets) also register a required health check when HealthModule (cap:health) is present; otherwise they fail fast on init errors.
- Capability ordering between modules is handled by each module's descriptor (requires/optional_requires). See docs/arch-design.md for details on module layering.
- Optional health integration: db, redis, secrets, and winreg modules declare an optional requirement on cap:health to participate in app readiness when HealthModule is present.

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

For crate README changes, follow the existing crate README structure (Overview, Compatibility, Dependencies, Setup, Usage, Status) and keep the root "Crate Status" table in sync with crate READMEs.

## Examples
- Configuration CLI example:
  - `cargo run -q -p airframe_config --example config_cli --features args`
- Logging CLI example:
  - `cargo run -q -p airframe_logging --example log_cli --features args`
  - Add `--features adapters-journald` or `--features adapters-syslog` to try platform sinks (where supported)
 - Secrets fallback example:
   - `cargo run -q -p airframe_secrets --example secret_fallback`

## Testing Strategy

We follow Rust testing conventions across crates:

- Unit tests: colocated with the implementation in each module using `#[cfg(test)] mod tests { ... }`. These can test private details.
- Integration tests: crate-level tests that exercise the public API live under each crate’s `tests/` directory (e.g., `crates/<crate>/tests/`). Files here compile as separate crates and should only use public APIs.
- Shared test helpers: if a crate needs shared fixtures/helpers for integration tests, place them in `tests/common/` and include with `mod common;` per test file.

Example: in `airframe_codec`, the codec round-trip tests have been moved to `crates/airframe_codec/tests/codec_roundtrip.rs`, while module-specific unit tests remain next to their modules.


## Bootstrap (early logging)

Airframe supports an optional bootstrap layer to capture logs emitted very early in the process, before your main logging module initializes. Opt in at app startup with `AppBuilder::with_bootstrap(...)` in `airframe_core`. Examples will adopt this incrementally as the logging integration evolves.
