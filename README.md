# Airframe

Airframe is a modular, security-focused Rust application framework. You compose
an application from small capability-providing modules; Airframe resolves their
initialization order, wires them together through a shared service registry, and
keeps all cryptography behind a single provider boundary.

It suits server services, long-running daemons, and CLI tools that handle
sensitive data and want a clean separation between application logic and the
cryptographic and storage primitives underneath.

## Highlights

- **Modular wiring** — each module declares what it *provides* and *requires*;
  Airframe topologically orders startup and shares services via a registry
  (`airframe_core`).
- **Cryptography at a boundary** — symmetric/asymmetric encryption, hashing,
  KDFs, and key wrapping live behind provider traits (`airframe_crypt`);
  applications never hand-roll crypto.
- **Layered storage** — composable key-value stores, caches, and codecs, plus a
  protected-at-rest pipeline (`airframe_data`, `airframe_pdata`, `airframe_sdata`).
- **Batteries, optional** — config, structured logging, HTTP, scheduling,
  health, metrics, audit logging, and database adapters, each its own crate.

## Installation

Add the crates you need:

```toml
[dependencies]
airframe_core = "1.0"
airframe_crypt = "1.0"
airframe_data = "1.0"
```

## Quick start

```rust
use airframe_core::app::AppBuilder;
use airframe_config::ConfigModule;
use airframe_logging::LoggingModule;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Compose the app from modules. Startup order is derived from the
    // capabilities each module declares — you don't wire it by hand.
    let app = AppBuilder::new()
        .with(ConfigModule::new(None))
        .with(LoggingModule::new())
        .start()
        .await?;

    // Resolved services are available from the registry, e.g.
    // let cfg = app.services.get::<airframe_config::api::types::BasicConfig>();
    let _ = app;
    Ok(())
}
```

Most crates ship runnable examples under `examples/`:

```bash
cargo run -p airframe_config  --example config_cli      --features args
cargo run -p airframe_logging --example log_cli         --features args
cargo run -p airframe_secrets --example secret_fallback
```

## Architecture

Airframe uses a strict layer model (L0–L7); dependencies only ever point
downward. See [docs/arch-design.md](docs/arch-design.md) for the layering rules,
the module/capability system, and the `RouterContributor` seam that lets HTTP
modules contribute routes without the server depending on them.

## Crates

Each crate has its own README with a consistent layout (overview, module
compatibility, dependencies, setup, usage).

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
- [airframe_health](crates/airframe_health/README.md) - Health checks (readiness/liveness); HTTP probe routes under the `http` feature
- [airframe_http](crates/airframe_http/README.md) - HTTP client/server adapters and module
- [airframe_id](crates/airframe_id/README.md) - Shared identifier types (InstallId, CeremonyId, Threshold, etc.)
- [airframe_ipc](crates/airframe_ipc/README.md) - IPC primitives: shared memory, Unix sockets, and child processes
- [airframe_kv](crates/airframe_kv/README.md) - Key-value store abstraction and adapters
- [airframe_log_api](crates/airframe_log_api/README.md) - Lightweight logging API: Logger trait, global setter, and no-op macros
- [airframe_logging](crates/airframe_logging/README.md) - Structured logging facade and sinks
- [airframe_macros](crates/airframe_macros/README.md) - Declarative macros for Airframe module development
- [airframe_metrics](crates/airframe_metrics/README.md) - Metrics primitives (Counter, Gauge, DurationMetric) with Prometheus exposition
- [airframe_mysql](crates/airframe_mysql/README.md) - MySQL database adapter
- [airframe_net](crates/airframe_net/README.md) - Async reliable UDP networking primitives
- [airframe_pdata](crates/airframe_pdata/README.md) - Protected-data (CtE) pipeline on top of data
- [airframe_pg](crates/airframe_pg/README.md) - PostgreSQL adapter for Airframe DB abstractions
- [airframe_prefab](crates/airframe_prefab/README.md) - Prefab presets for CLI and service apps
- [airframe_recovery_bundle](crates/airframe_recovery_bundle/README.md) - Recovery bundle format and K-of-N share-combining
- [airframe_redis](crates/airframe_redis/README.md) - Redis adapter and module with optional health
- [airframe_scheduler](crates/airframe_scheduler/README.md) - Job scheduler and timers
- [airframe_sdata](crates/airframe_sdata/README.md) - Schema-aware typed repos and caches
- [airframe_secrets](crates/airframe_secrets/README.md) - Secret resolution interfaces and in-memory cache
- [airframe_sqlite](crates/airframe_sqlite/README.md) - SQLite database adapter
- [airframe_tabular](crates/airframe_tabular/README.md) - Config-driven tabular ingest (CSV/TSV) to typed rows
- [airframe_winreg](crates/airframe_winreg/README.md) - Windows registry-based ByteCache provider
- [airframe_wire](crates/airframe_wire/README.md) - Bit-level binary protocol primitives

## Contributing

Contributions — including AI-assisted ones — are welcome. Start with
[CONTRIBUTING.md](CONTRIBUTING.md). If you use an LLM or coding agent, point it
at [AGENTS.md](AGENTS.md): it spells out the gates, the layering rules, the
security model, and the verification a pull request must meet.

## License

Licensed under the [MIT License](LICENSE).
