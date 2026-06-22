// Negative CORS integration test to ensure disallowed origins are not permitted.
// Run with:
//   cargo test -p airframe_prefab --features http --test http_cors_negative -- --nocapture

#[cfg(feature = "http")]
mod http_cors_negative {
    use std::fs::File;
    use std::io::Write;
    use std::time::Duration;

    use airframe_core::app::AppBuilder;
    use airframe_http::axum_server::BoundAddr;
    use tempfile::tempdir;

    #[tokio::test]
    async fn cors_preflight_blocks_unlisted_origin() {
        // Create a temp config file enabling CORS with a specific origin only
        let dir = tempdir().expect("tempdir");
        let cfg_path = dir.path().join("cors.toml");
        let mut f = File::create(&cfg_path).expect("create config file");
        writeln!(
            f,
            r#"
[cors]
enable = true
allow_origins = ["https://example.com"]
allow_methods = ["GET", "OPTIONS"]
allow_headers = ["X-Test"]
"#
        )
        .expect("write config");
        drop(f);

        // Build the app manually (same modules as HttpApiServerPrefab) but pass
        // the config path directly instead of via AIRFRAME_CONFIG_PATH env var.
        // This avoids cross-binary env var races that cause flaky failures in
        // parallel test runs (e.g. cargo llvm-cov --workspace).
        let bind: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
        let app = AppBuilder::new()
            .with_bootstrap(airframe_core::app::Bootstrap {
                minimal_logger: true,
            })
            .with(airframe_health::HealthModule::new())
            .with(airframe_prefab::http_cors::HttpCorsModule::new())
            .with(
                airframe_config::ConfigModule::new(Some(cfg_path.clone()))
                    .with_defaults(airframe_prefab::http_api())
                    .with_hot_reload(false),
            )
            .with(airframe_http::axum_server::AxumServerModule::new(bind))
            .start()
            .await
            .expect("app should start");

        // Discover bound address
        let addr = app
            .services
            .get::<BoundAddr>()
            .expect("BoundAddr present")
            .0;

        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("client");

        let url = format!("http://{}/health", addr);

        // Readiness loop and then check headers for a disallowed Origin
        let mut ok = false;
        for _ in 0..50 {
            let req = client
                .request(reqwest::Method::OPTIONS, &url)
                .header("Origin", "https://evil.com")
                .header("Access-Control-Request-Method", "GET")
                .build()
                .unwrap();
            match client.execute(req).await {
                Ok(resp) => {
                    // Should not echo back evil origin; header must be absent
                    let allow_origin = resp.headers().get("access-control-allow-origin").cloned();
                    assert!(
                        allow_origin.is_none(),
                        "unexpected allow-origin for disallowed origin"
                    );
                    ok = true;
                    break;
                }
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
        if !ok {
            panic!("server did not respond within timeout");
        }

        let mut app = app;
        app.shutdown().await.expect("shutdown ok");
    }
}

#[cfg(not(feature = "http"))]
#[test]
fn http_feature_required() {
    // No-op: required features are not enabled
}
