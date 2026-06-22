// This example requires the `args` feature to be enabled.

#[cfg(feature = "args")]
use airframe_args::ArgsModuleWithStartup;
#[cfg(feature = "args")]
use airframe_config::{BasicConfig, ConfigModule};
#[cfg(feature = "args")]
use airframe_core::app::AppBuilder;
#[cfg(feature = "args")]
use airframe_logging::{LoggingModule, LoggingState};
#[cfg(feature = "args")]
use clap::Parser;

#[cfg(feature = "args")]
#[derive(Parser, Debug)]
struct Cli {}

#[cfg(feature = "args")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Wire Args -> Config -> Logging (Pattern A)
    let app = AppBuilder::new()
        .with(ArgsModuleWithStartup::new())
        .with(ConfigModule::new(None))
        .with(LoggingModule::new())
        .start()
        .await?;

    let state = app.services.get::<LoggingState>().unwrap();
    let cfg = state.get();
    println!("Logging directives: {:?}", cfg.directives);
    if let Some(bc) = app.services.get::<BasicConfig>() {
        println!("Config source: {:?}", bc.source);
    }

    // Try some logs
    tracing::trace!(target = "example", "trace line");
    tracing::debug!(target = "example", "debug line");
    tracing::info!(target = "example", "info line");
    tracing::warn!(target = "example", "warn line");
    tracing::error!(target = "example", "error line");

    Ok(())
}

#[cfg(not(feature = "args"))]
fn main() {
    eprintln!("This example requires the 'args' feature: cargo run -p airframe_logging --example log_cli --features args");
}
