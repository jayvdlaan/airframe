// Example: Print merged server.bind from defaults, file, env, and CLI overrides.
// Requires features: module + args (so that --config/--cfg.* are honored via ArgsModule).

use airframe_args::ArgsModule;
use airframe_config::{api::types::BasicConfig, ConfigModule};
use airframe_core::app::AppBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Provide a sensible default bind address that can be overridden by file/env/CLI
    let defaults: toml::Value = "[server]\nbind='127.0.0.1:8080'\n".parse().unwrap();

    // Compose Args + Config. Config will read paths from --config/--config-path when present
    // and apply CLI overrides like --cfg.server.bind=127.0.0.1:9000
    let app = AppBuilder::new()
        .with(ArgsModule::new())
        .with(
            ConfigModule::new(None)
                .with_defaults(defaults)
                .with_hot_reload(false),
        )
        .start()
        .await?;

    if let Some(cfg) = app.services.get::<BasicConfig>() {
        let bind = cfg
            .raw
            .get("server")
            .and_then(|t| t.get("bind"))
            .and_then(|v| v.as_str())
            .unwrap_or("(unset)");
        println!("server.bind = {}", bind);
    } else {
        println!("No configuration available");
    }

    Ok(())
}
