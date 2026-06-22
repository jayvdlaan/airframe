// Example: Parse --config path(s) and override settings with --cfg.* flags.
// Demonstrates integration between airframe_args and airframe_config.

use airframe_args::ArgsModule;
use airframe_config::{api::types::BasicConfig, ConfigModule};
use airframe_core::app::AppBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Compose Args + Config. Config will read paths from --config/--config-path when present
    // and apply CLI overrides like --cfg.app.name=myapp
    let app = AppBuilder::new()
        .with(ArgsModule::new())
        .with(ConfigModule::new(None))
        .start()
        .await?;

    if let Some(cfg) = app.services.get::<BasicConfig>() {
        println!(
            "Merged config (TOML):\n{}",
            toml::to_string_pretty(&cfg.raw).unwrap()
        );
        if let Some(src) = &cfg.source {
            println!("Source file: {}", src.display());
        }
    } else {
        println!("No config loaded");
    }

    Ok(())
}
