# airframe_logging

Dynamically reconfigurable, multi-sink logging for Airframe, built on `tracing` and `tracing-subscriber` with hot-reload, rotation, per-sink filters, and correlation IDs.

## Overview

This crate provides a logging module for Airframe applications that can be configured at runtime and reloaded on configuration changes. It multiplexes logs to multiple sinks (console/file/platform), supports per-sink filters and formatting, rotation (time-based and size-based), and optional correlation ID injection. It integrates with Airframe configuration to reload settings without restarting the app.

## Logical pieces

- LoggingModule: Airframe module that wires logging into the runtime and listens for config changes
- LoggingState: Snapshot of current logging configuration and diagnostics registered in the ServiceRegistry
- Sinks: Console and file sinks built-in; optional platform sinks behind features (`adapters-journald`, `adapters-syslog`)
- Filters: Global EnvFilter (reloadable) and per-sink filter strings
- Rotation: Daily/hourly or size-based rotation with retention
- Correlation IDs: task-local helpers to inject request/trace IDs into events
- Events and controls: EventBus messages to update filters, sinks, and formats at runtime
- Testing utilities: deterministic in-memory sink and helpers for unit tests

## Airframe module compatibility

- Compatibility: Yes — provides `cap:logging`
- Requires: `cap:config` (consumes configuration from `[logging]`), optionally `cap:args` for CLI integration when feature `args` is enabled

## Dependencies

- Rust dependencies/features: see Cargo.toml
  - Features: `args` (CLI integration), `adapters-journald` (systemd journal sink), `adapters-syslog` (syslog sink)
- System libraries:
  - journald: requires libsystemd on Linux when `journald` feature is enabled
  - syslog: requires platform syslog support (varies by OS)
- Airframe capacities/modules: Exports `cap:logging` via LoggingModule and reads configuration via `cap:config`

## Setup / Installation

Add the module and config to your app:

```toml
[dependencies]
airframe_core = { path = "../airframe_core" }
airframe_config = { path = "../airframe_config" }
airframe_logging = { path = "../airframe_logging" }
```

Optional features:

```toml
[dependencies]
# CLI integration to tweak logging at startup
airframe_args = { path = "../airframe_args" }
airframe_logging = { path = "../airframe_logging", features = ["args"] }
# Platform sinks
# airframe_logging = { path = "../airframe_logging", features = ["adapters-journald", "adapters-syslog"] }
```

- Provides capability `cap:logging`, requires `cap:config`.
- Installs a reloadable global subscriber (falls back to thread-local in tests) with:
  - Global EnvFilter (reloadable) built from `[logging].directives`.
  - Composite Sinks layer multiplexing to N sinks (console/file) with per-sink filters and formatting.
  - Time-based rotation (daily/hourly) and size-based rotation with retention.
  - Correlation ID injection (text/JSON) when enabled per sink.
- Exposes `LoggingState` snapshot and runtime controls via EventBus.

## Configuration (sinks-first schema)
Legacy top-level keys (level, env_filter, targets, json, ansi) are rejected. Use the sinks-first schema:

```toml
[logging]
# Baseline directives that build the global EnvFilter
# Example: allow info globally and debug for a module
# directives = ["info", "my_crate::db=debug"]
directives = ["info"]

[[logging.sinks]]
kind = "console"
json = true
ansi = false
filter = "airframe_api=info"
# Optional per-sink formatting
format = { with_span_events = "enter", target = true, level = true, include_correlation_id = true }

[[logging.sinks]]
kind = "file"
path = "logs/app.log"
json = false
ansi = false
# rotation can be "never" (default), "hourly", "daily", or size-based
rotation = { policy = "size", max_bytes = 32768, keep = 5 }
filter = "info"
format = { target = true, level = true, include_correlation_id = true }
```

Precedence: events must pass the global EnvFilter first, then each sink's per-sink filter.

## Size-based rotation with retention
- Configure with `rotation = { policy = "size", max_bytes = <u64>, keep = <usize> }`.
- Naming scheme: base, base.1, base.2, ..., base.N (older pruned beyond `keep`).
- Works with non-blocking writers; flushes on module stop.

## Correlation IDs
Task-local helpers allow injecting a correlation ID into every event when enabled by the sink format:

```rust
use airframe_logging::correlation;

async fn handle_request() {
    correlation::scope("req-123", async move {
        tracing::info!(target = "airframe_api", "processing");
    }).await;
}
```

- Per-sink `format.include_correlation_id` (default true) enables injection.
- Text outputs add a `[correlation_id=…]` prefix; JSON outputs include a top-level `correlation_id` field.

## Runtime controls (EventBus)
- Set or update global filter directives:
  - `SetLogFilter { filter: String }`
  - `SetLogLevel { target: Option<String>, level: String }`
- Toggle formatting:
  - `ToggleJson { enabled: bool }`, `SetAnsi { enabled: bool }`
- Manage sinks:
  - `AddSink { sink: SinkConfig }`, `RemoveSink { sink_id: usize }`
  - `SetSinkFilter { sink_id, filter: Option<String> }`
  - `SetSinkFormat { sink_id, json, ansi, with_span_events, include_correlation_id }`
- Diagnostics:
  - Publish `RequestLoggingStatus` to receive `LoggingStatus { config, global_filter, sinks: [SinkDiag…] }`.

## Testing utilities
Deterministic buffer sink for unit tests:

```rust
use airframe_logging::testing;

#[test]
fn captures_text() {
    let _g = testing::init_for_test("info", false);
    tracing::info!(target: "my", "hello");
    let out = testing::take();
    assert!(out.contains("hello"));
}

#[test]
fn captures_json() {
    let _g = testing::init_for_test("info", true);
    tracing::info!(target: "my", msg = "hello");
    let out = testing::take();
    assert!(out.contains("\"level\""));
}
```

## Usage
Note: The sinks-first schema is required; at least one sink must be configured. There is no legacy fallback.

```rust
use airframe_core::app::AppBuilder;
use airframe_config::ConfigModule;
use airframe_logging::{LoggingModule, LoggingState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new()
        .with(ConfigModule::new(None))
        .with(LoggingModule::new())
        .start()
        .await?;

    // Logs now flow to the configured sinks. Inspect current config:
    let state = app.services.get::<LoggingState>().unwrap();
    println!("global filter: {:?}", state.get().directives);
    Ok(())
}
```

### Example
- Run the CLI example demonstrating Args + Config + Logging and trying different outputs:
  - `cargo run -q -p airframe_logging --example log_cli --features args`
  - Add `--features journald` or `--features syslog` to try platform sinks (where supported).

### CLI integration (feature `args`)
When built with feature `args` and wired with `airframe_args::ArgsModuleWithStartup`, logging accepts dedicated flags in addition to `--cfg.logging.*` config overrides.

- Wiring
```rust
use airframe_core::app::AppBuilder;
use airframe_args::ArgsModuleWithStartup;
use airframe_config::ConfigModule;
use airframe_logging::LoggingModule;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new()
        .with(ArgsModuleWithStartup::<()>::new()) // provides cap:args
        .with(ConfigModule::new(None))             // requires cap:args under its own feature
        .with(LoggingModule::new())                // requires cap:args + cap:config under feature
        .start()
        .await?;
    Ok(())
}
```

- Flags
  - `--log-level <trace|debug|info|warn|error>`
  - `--log-filter <env_filter_spec>` (e.g., `mycrate=debug,info`)
  - `--log-json[=true|false]`
  - `--log-output <spec>` where `<spec>` is one of:
    - `stdout`
    - `stderr`
    - `file:<path>`
    - `roll:daily:<dir>:<name>`
    - `roll:hourly:<dir>:<name>`
    - `roll:size:<path>:<max_bytes>:<keep>`
    - `journald` (requires feature `adapters-journald`)
    - `syslog` (requires feature `adapters-syslog`; platform support may vary)

Precedence for bootstrap knobs: CLI flags → `[logging]` config → module defaults. CLI overrides are re-applied on config reloads.

Global options in `[logging]`:
- `non_blocking_buffer_lines = <usize>`: buffer size (in lines) for non-blocking file writers. Useful to prevent drops under bursty load.

## Notes
- If a global subscriber is already set, the module falls back to a thread-local subscriber with matching reload handles.
- Legacy keys are rejected with a descriptive error pointing to the sinks-first schema.
