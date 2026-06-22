use airframe_config::ConfigModule;
use airframe_core::app::AppBuilder;
use airframe_core::bus::EventBus; // bring trait into scope for subscribe()
use airframe_log_api as log_api;
use airframe_logging::{LoggingChanged, LoggingModule, LoggingState};
use std::time::Duration;
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Prepare a minimal sinks-first logging config writing to console
    let dir = tempfile::tempdir()?;
    let cfg_path = dir.path().join("logging.toml");
    std::fs::write(
        &cfg_path,
        r#"[logging]

directives=["info"]

[[logging.sinks]]
kind="console"
json=false
ansi=true
filter="airframe_logging=info,airframe_log_api=info"
"#,
    )?;

    let app = AppBuilder::new()
        .with(ConfigModule::new(Some(cfg_path.clone())))
        .with(LoggingModule::new())
        .start()
        .await?;

    let events = app.events.clone();
    let mut changed = events.subscribe::<LoggingChanged>()?;

    if let Some(state) = app.services.get::<LoggingState>() {
        println!("initial directives: {:?}", state.get().directives);
    }

    // Emit a message via airframe_log_api macros; this should be forwarded into tracing
    log_api::info!("hello from airframe_log_api bridge");

    // Change config and wait for LoggingChanged
    std::fs::write(
        &cfg_path,
        r#"[logging]

directives=["debug"]

[[logging.sinks]]
kind="console"
json=false
ansi=true
filter="airframe_logging=debug,airframe_log_api=debug"
"#,
    )?;
    let _ = tokio::time::timeout(Duration::from_secs(2), changed.next()).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;

    if let Some(state) = app.services.get::<LoggingState>() {
        println!("updated directives: {:?}", state.get().directives);
    }

    app.cancel.cancel();
    Ok(())
}
