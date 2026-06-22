use std::fs;
use std::time::Duration;

use airframe_config::api::types::BasicConfig;
use airframe_core::app::AppBuilder;

#[tokio::test]
async fn cli_overrides_take_precedence_over_file() {
    // Prepare a temp config file with a value we will override via CLI
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.toml");
    fs::write(&path, "[app]\nname='from_file'\n").unwrap();

    // Build app with ArgsModule (to exercise integration presence) and ConfigModule
    // Inject CLI overrides using ConfigModule API to simulate argv-sourced overrides.
    let builder = AppBuilder::new()
        .with(airframe_args::ArgsModule::new())
        .with(
            airframe_config::ConfigModule::new(Some(path.clone()))
                .with_cli_overrides(vec!["--cfg.app.name=from_cli".to_string()])
                .with_hot_reload(false),
        );

    let app = builder.start().await.expect("app start");

    // Fetch merged config and assert CLI override wins over file value
    let cfg = app
        .services
        .get::<BasicConfig>()
        .expect("BasicConfig present");
    let t = cfg.raw.as_table().unwrap();
    let app_t = t.get("app").and_then(|v| v.as_table()).unwrap();
    assert_eq!(app_t.get("name").and_then(|v| v.as_str()), Some("from_cli"));

    // Ensure argv captured still present unknown flags for forward-compat
    let args = app
        .services
        .get::<airframe_args::CliArgs>()
        .expect("CliArgs present");
    // We didn't actually set process args in this test; just ensure no panic and that argv is a Vec
    assert!(args.argv.len() <= args.raw.len().saturating_sub(1));

    // Graceful shutdown
    app.cancel_after(Duration::from_millis(10));
}
