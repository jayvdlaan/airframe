# airframe_prefab

Prefab builders for common application types (CLI, services, HTTP servers, workers) on top of Airframe modules. Each prefab returns an AppBuilder pre-wired with sensible defaults so you can start quickly and opt in to additional modules as needed.

Status: The following prefabs are implemented and ready to use today.

Implemented prefabs
- CliPrefab — terminal apps (optionally: args, config, logging)
- ServicePrefab — long-running daemon (optionally: config, logging) with health
- WorkerPrefab — background consumer (optionally: config, logging) with health
- ScheduledServicePrefab — cron-like jobs with scheduler and health (optional config/logging)
- HttpApiServerPrefab — HTTP API server with health and optional CORS/OpenAPI (feature-gated)
- GatewayPrefab — reverse proxy/gateway with routing and optional CORS (feature-gated)

Feature flags
- args: include airframe_args and enable CLI args parsing in prefabs
- config: include airframe_config and load prefab defaults (TOML) and files/env
- logging: include airframe_logging and install structured logging module
- http: enables HTTP-based prefabs (HttpApiServerPrefab, GatewayPrefab) and related modules
- openapi: optional, augments HttpApiServerPrefab with OpenAPI contributor when combined with http
- swagger-ui: optional, serves Swagger UI for OpenAPI (implies openapi)

Quick start

Add to Cargo.toml
```toml
[dependencies]
airframe_prefab = { path = "../../crates/airframe_prefab" }
```

Minimal usage (CLI)
```rust
use airframe_prefab::CliPrefab;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = CliPrefab::new().start().await?; // modules started
    // Your CLI dispatch goes here
    app.shutdown().await?;
    Ok(0.into())
}
```

Runnable examples
- CLI (with args+config+logging): cargo run -p airframe_prefab --features args,config,logging --example cli -- hello World 2>logs.txt
- Service (with logging): cargo run -p airframe_prefab --features logging --example service 2>logs.txt
- Worker (with logging): cargo run -p airframe_prefab --features logging --example worker 2>logs.txt
- HTTP API: cargo run -p airframe_prefab --features http,logging,config --example http_api 2>logs.txt
- Gateway: cargo run -p airframe_prefab --features http,config,logging --example gateway -- --config ./gateway.toml 2>logs.txt

Notes
- Prefabs install a minimal early stderr logger during bootstrap so very-early logs are captured before the full logger initializes. Redirect stderr to a file if you want to keep these logs (e.g., 2>logs.txt).
- Defaults such as logging and config can be overridden via config files, environment variables, or args (when those features are enabled). See each module’s README for details.

References
- Full prefab documentation, configuration, and checklists: ../../docs/prefabs.md
- Core runtime and AppBuilder: ../../crates/airframe_core/README.md
- HTTP module (client/server): ../../crates/airframe_http/README.md
- Logging module: ../../crates/airframe_logging/README.md
- Config module: ../../crates/airframe_config/README.md

## License

Licensed under the MIT License.
