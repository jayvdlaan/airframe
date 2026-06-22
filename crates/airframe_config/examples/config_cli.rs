use airframe_args::ArgsModuleWithStartup;
use airframe_config::{BasicConfig, ConfigModule};
use airframe_core::app::AppBuilder;
use clap::Parser;

#[derive(Parser, Debug)]
struct Cli {}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // With feature "args" enabled on airframe_config, ConfigModule requires cap:args and
    // can read --config/--config-path from the CLI.
    let app = AppBuilder::new()
        .with(ArgsModuleWithStartup::new())
        .with(ConfigModule::new(None))
        .start()
        .await?;

    let cfg = app.services.get::<BasicConfig>().unwrap();
    println!("Config source: {:?}", cfg.source);
    Ok(())
}
