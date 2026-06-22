# airframe_args

Captures and normalizes process argv into a structured `CliArgs` service, with small argv-parsing helpers shared across Airframe modules.

## Overview

`airframe_args` provides an Airframe module that collects `std::env::args()`, normalizes it, and registers a `CliArgs` value in the `ServiceRegistry`. Other modules (for example `airframe_config` with its `args` feature) can require `cap:args` to guarantee ordering and read the captured CLI.

Normalization (`CliArgs::new_normalized` / `normalize_argv`) does the following:

- Drops the program name (`raw[0]`) to produce `argv`.
- Records whether a literal `--` separator was present (`has_separator`).
- Collects leading global flags into `GlobalFlags`: `--quiet`, `--verbose`, `--json`. Any other leading `-`/`--` token is preserved in `globals.passthrough` for forward compatibility.
- Identifies the first positional token after the globals (or after `--`) as `command`, with the remaining tokens as `command_args`.
- Builds `argv` (used by the config/overrides layer) from all post-program tokens, excluding the `--` separator token itself when present.

Public items:

- `CliArgs` — the normalized result registered into the `ServiceRegistry`. Fields: `argv: Vec<String>`, `raw: Vec<String>`, `has_separator: bool`, `globals: GlobalFlags`, `command: Option<String>`, `command_args: Vec<String>`. Constructed via `CliArgs::new_normalized(raw)`.
- `GlobalFlags` — `quiet`, `verbose`, `json`, and `passthrough: Vec<String>`.
- `ArgsModule` — the module that normalizes argv and registers `CliArgs` (provides `cap:args`).
- `ArgsModuleWithStartup` — wraps `ArgsModule`; in addition to registering `CliArgs`, it publishes an `AppStartup` event when an `InMemoryEventBus` is present in the registry. This type is **not** generic and performs no clap parsing.
- `AppStartup` — a zero-sized `Event` (`NAME = "AppStartup"`) signalling that argv has been captured.
- `cli` module — argv-parsing helpers:
  - `cli::get_value(argv, long) -> Option<String>` — value for a long option, accepting both `--name=value` and `--name value`.
  - `cli::get_bool(argv, long) -> Option<bool>` — boolean for a long flag, accepting `--flag`, `--flag=true/false`, and `--flag true/false` (treats `false`/`0`/`no`/`off` as false).
  - `cli::split_paths_os_portable(s) -> Vec<PathBuf>` — OS-portable path-list split (`,`/`;` everywhere, plus `:` on Unix).

There is no generic clap-parsing module in this crate; argument interpretation is left to consumers via `CliArgs` and the `cli` helpers.

## Airframe module compatibility

- Compatibility: Yes — `ArgsModule` and `ArgsModuleWithStartup` provide `cap:args`.
- Platform support: desktop only (CLI args are not supported on mobile targets).
- Access: `app.services.get::<airframe_args::CliArgs>()` returns `Option<Arc<CliArgs>>`.

## Dependencies

- Airframe crates: `airframe_core` (L0), `airframe_macros`.
- Rust dependencies: `clap`, `serde`, `anyhow`, `tracing`, `async-trait`, `semver` (see `Cargo.toml`).
- System libraries: none.
- Capabilities exported: `cap:args`.

## Usage

Wire `ArgsModule` into an app and read the normalized `CliArgs`, then use the `cli` helpers to interpret options:

```rust
use airframe_args::{cli, ArgsModule, CliArgs};
use airframe_core::app::AppBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new().with(ArgsModule::new()).start().await?;

    let args = app
        .services
        .get::<CliArgs>()
        .expect("CliArgs registered by ArgsModule");

    println!("command: {:?}", args.command);
    println!("verbose: {}", args.globals.verbose);

    // Interpret a long option from the normalized argv.
    if let Some(path) = cli::get_value(&args.argv, "--config") {
        println!("config path: {path}");
    }
    if cli::get_bool(&args.argv, "--dry-run").unwrap_or(false) {
        println!("dry run enabled");
    }

    app.shutdown().await
}
```

To additionally publish an `AppStartup` event once argv is captured, use `ArgsModuleWithStartup::new()` in place of `ArgsModule::new()`; it registers the same `CliArgs` and emits `AppStartup` when an in-memory event bus is registered.

The `cli` helpers can also be used directly, independent of the module:

```rust
use airframe_args::cli;

let argv: Vec<String> = vec!["--name=alice".into(), "--flag".into()];
assert_eq!(cli::get_value(&argv, "--name").as_deref(), Some("alice"));
assert_eq!(cli::get_bool(&argv, "--flag"), Some(true));
```

## Status

Pre-release: 0.5.0-beta.

Licensed under MIT.
