# airframe_config

Short description: Layered TOML configuration for Airframe apps, with optional module integration and hot-reload.

## Overview

airframe_config builds a layered configuration and makes it available to your app as a typed `BasicConfig`. When used as an Airframe module (feature `module`), it registers `BasicConfig` in the `ServiceRegistry` and publishes a `ConfigReloaded` event so subscribers can react to changes.

## Logical pieces

- BasicConfig: typed holder for the merged configuration and optional `source` path
- ConfigModule (feature `module`): integrates with the Airframe runtime; selects files, loads/merges config, registers `BasicConfig`, and publishes `ConfigReloaded`
- Events: `ConfigReloaded` indicates that configuration changed/reloaded
- Resolver: determines file selection and precedence from constructor defaults, environment, and CLI
- Overrides: environment variables `AIRFRAME__...` and CLI flags `--cfg.section.key=value` (with feature `args`)
- Builder options: `.with_hot_reload(bool)`, `.with_strict_file_selection(bool)`

## Airframe module compatibility

- Compatibility: Yes (with feature `module`)
- Provides: registers `BasicConfig` in the ServiceRegistry and publishes `ConfigReloaded`

## Dependencies

- Rust dependencies/features: see Cargo.toml
  - `module`: enables module integration and file watching
  - `args`: enable CLI integration (requires `airframe_args`)
- System libraries: none
- Airframe capacities/modules: Provides configuration capability via `BasicConfig` and `ConfigReloaded` when used as a module (no standardized cap name).

## Setup / Installation

Add the dependency (library-only):

```toml
[dependencies]
airframe_config = { path = "../airframe_config" }
```

As an Airframe module:

```toml
[dependencies]
airframe_config = { path = "../airframe_config", features = ["module"] }
```

To parse CLI flags for file selection and overrides:

```toml
[dependencies]
airframe_args = { path = "../airframe_args" }
airframe_config = { path = "../airframe_config", features = ["module", "args"] }
```

## Usage

### Example 1: Wire the module and read BasicConfig

```rust
use airframe_core::app::AppBuilder;
use airframe_config::{ConfigModule, BasicConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new()
        .with(ConfigModule::new(None))
        .start()
        .await?;
    if let Some(cfg) = app.services.get::<BasicConfig>() {
        println!("config loaded from: {:?}", cfg.source);
    }
    Ok(())
}
```

Run the bundled example:
- cargo run -q -p airframe_config --example config_basic --features module

### Example 2: Select config via CLI and environment, with hot reload

With features `module, args` and `ArgsModule` in your app, you can select files via `--config`/`--config-path`, or via `AIRFRAME_CONFIG_PATH`.

```rust
use airframe_core::app::AppBuilder;
use airframe_args::ArgsModuleWithStartup; // provides cap:args + CliArgs
use airframe_config::ConfigModule;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new()
        .with(ArgsModuleWithStartup::<()>::new())
        .with(ConfigModule::new(None))
        .start()
        .await?;
    // Listen for reload events or just read BasicConfig when needed
    Ok(())
}
```

Try it:
- Single file with hot reload: cargo run -q -p airframe_config --example config_cli --features args -- --config ./app.toml
- Multiple files (merge, no hot reload): cargo run -q -p airframe_config --example config_cli --features args -- --config ./base.toml:./local.toml

Selection rules (last wins):
- defaults (if supplied via `ConfigModule::new(Some(path))`)
- file(s) selected via CLI/env/default
- environment overrides: variables prefixed with `AIRFRAME__` (e.g., `AIRFRAME__logging__level=debug`)
- CLI overrides: `--cfg.section.key=value` (when built with the `args` feature)

Precedence: CLI `--config/--config-path` → `AIRFRAME_CONFIG_PATH` env var → constructor `default_path`.

Hot-reload:
- Enabled only when exactly one file is selected; `BasicConfig.source` is set to that path and file changes trigger a reload.
- Disabled when multiple files are selected, or when explicitly turned off via the builder.

### Example 3: Print merged server.bind

This small example prints the final `server.bind` after applying defaults, files, env, and CLI overrides. Handy for verifying precedence (Args > Env > File > Defaults).

Run:

```
cargo run -q -p airframe_config --example print_bind --features args -- \
  --cfg.server.bind=127.0.0.1:9000
```

You can also point to a file with `--config ./app.toml` or via `AIRFRAME_CONFIG_PATH=./app.toml`.

## License

Licensed under the MIT License.
