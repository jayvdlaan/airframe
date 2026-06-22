#[cfg(feature = "module")]
#[tokio::test]
async fn db_module_starts() {
    use airframe_core::app::AppBuilder;
    use airframe_db::DbModule;
    use airframe_health::ServiceRegistryHealthExt;

    let mut app = AppBuilder::new()
        .with(airframe_health::HealthModule::new())
        .with(DbModule::new())
        .start()
        .await
        .unwrap();

    // ensure a health check named "db" is registered
    let health = app.services.health().expect("HealthService present");
    let names: Vec<String> = health
        .checks_snapshot()
        .into_iter()
        .map(|(n, _, _)| n)
        .collect();
    assert!(names.iter().any(|n| n == "db"));

    app.shutdown().await.unwrap();
}
