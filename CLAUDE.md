# Airframe

A modular Rust SDK providing core functionality for secure data handling, cryptographic operations, and storage solutions.

## Architecture

Airframe uses a layered module system (L0-L7) with strict dependency rules. See `docs/arch-design.md` for details.

**Key rule**: Dependencies must only point downward. A crate at layer N may depend on layers <= N.

### Layers

- **L0 Core**: `airframe_core` — Module system, ServiceRegistry
- **L1 Primitives**: `airframe_codec`, `airframe_crypt`, `airframe_compress`, `airframe_api`, `airframe_macros`, `airframe_log_api`, `airframe_id`, `airframe_wire`, `airframe_net`, `airframe_channel`, `airframe_pdata`, `airframe_sdata`
- **L2 Config/Args**: `airframe_config`, `airframe_args`
- **L3 Logging**: `airframe_logging`
- **L4 IO Adapters**: `airframe_http`, `airframe_kv`, `airframe_data`, `airframe_db`, `airframe_secrets`, `airframe_scheduler`, `airframe_event`, `airframe_health`, `airframe_redis`, `airframe_sqlite`, `airframe_mysql`, `airframe_pg`, `airframe_winreg`, `airframe_audit`, `airframe_metrics`, `airframe_tabular`, `airframe_ipc`
- **L5 Prefabs/Integrations**: `airframe_prefab`, `airframe_recovery_bundle`

## Crate Structure

All crates are in `crates/airframe_*`. Each has its own README with:
- Overview
- Airframe module compatibility
- Dependencies
- Setup and usage
- Status

## Module System

Modules declare capabilities via `ModuleDescriptor`:
- `provides` — Capabilities this module provides (e.g., `cap:http.server`)
- `requires` — Required capabilities for initialization
- `optional_requires` — Optional capabilities (still affects topology)

Prefer runtime `ServiceRegistry` lookups over module edges when ordering isn't required.

## Key Patterns

### Capability-Based Wiring
```rust
impl ModuleDescriptor for MyModule {
    fn provides(&self) -> &'static [&'static str] { &["cap:myservice"] }
    fn requires(&self) -> &'static [&'static str] { &["cap:config"] }
}
```

### RouterContributor
HTTP modules mount routes via `RouterContributor` seam without the HTTP server depending on them.

### Secrets
Use `airframe_secrets` for secret resolution. Never store plaintext secrets.

## Build & Test

```bash
# Build all crates
cargo build --workspace

# Run tests with layer validation
cargo test -p airframe_core --features layer-check

# Examples
cargo run -p airframe_config --example config_cli --features args
cargo run -p airframe_logging --example log_cli --features args
```

## Conventions

- Use `tracing` for logging, not direct dependencies on logging modules
- Prefer runtime ServiceRegistry lookups over descriptor edges for optional services
- Follow the layer model strictly — no upward dependencies
- Use feature flags consistently across all build configurations
