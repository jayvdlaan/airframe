use std::time::Duration;

use airframe_config::ConfigModule;
use airframe_core::app::AppBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let cfg_path = dir.path().join("app.toml");
    std::fs::write(&cfg_path, "[logging]\nlevel='warn'\n")?;

    let app = AppBuilder::new()
        .with(ConfigModule::new(Some(cfg_path.clone())))
        .start()
        .await?;

    if let Some(cfg) = app.services.get::<airframe_config::BasicConfig>() {
        let level = cfg
            .raw
            .get("logging")
            .and_then(|t| t.get("level"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        println!("logging.level = {}", level);
    }

    // Change config and allow debounce to pick it up
    std::fs::write(&cfg_path, "[logging]\nlevel='info'\n")?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    app.cancel.cancel();
    Ok(())
}
