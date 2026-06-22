// Integration test for configurable CORS on the HTTP API Server prefab.
// Run with:
//   cargo test -p airframe_prefab --features "http,config" --test http_cors -- --nocapture

#[cfg(all(feature = "http", feature = "config"))]
mod http_cors {
    use std::fs::File;
    use std::io::Write;
    use std::time::Duration;

    use airframe_core::app::AppBuilder;
    use airframe_http::axum_server::BoundAddr;
    use tempfile::tempdir;

    #[tokio::test]
    async fn cors_preflight_allows_configured_origin_and_method() {
        // Create a temp config file enabling CORS with a specific origin and method
        let dir = tempdir().expect("tempdir");
        let cfg_path = dir.path().join("cors.toml");
        let mut f = File::create(&cfg_path).expect("create config file");
        // Configure allowed origin and methods/headers explicitly
        writeln!(
            f,
            r#"
[cors]
enable = true
allow_origins = ["https://example.com"]
allow_methods = ["GET", "OPTIONS"]
allow_headers = ["X-Test"]
max_age = 600
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

        // Preflight to /health with configured Origin and Access-Control-Request-Method
        let url = format!("http://{}/health", addr);

        // Poll readiness
        let mut ok = false;
        for _ in 0..50 {
            let req = client
                .request(reqwest::Method::OPTIONS, &url)
                .header("Origin", "https://example.com")
                .header("Access-Control-Request-Method", "GET")
                .build()
                .unwrap();
            match client.execute(req).await {
                Ok(resp) => {
                    // Should be 204 No Content preflight or 200 OK, and include CORS headers
                    assert!(
                        resp.status().is_success(),
                        "unexpected status: {}",
                        resp.status()
                    );
                    let allow_origin = resp.headers().get("access-control-allow-origin").cloned();
                    assert!(
                        allow_origin.is_some(),
                        "missing access-control-allow-origin"
                    );
                    assert_eq!(allow_origin.unwrap(), "https://example.com");
                    let allow_methods = resp
                        .headers()
                        .get("access-control-allow-methods")
                        .and_then(|h| h.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    assert!(
                        allow_methods.contains("GET"),
                        "allow-methods missing GET: {}",
                        allow_methods
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
            panic!("server did not respond with expected CORS headers within timeout");
        }

        // Shutdown
        let mut app = app;
        app.shutdown().await.expect("shutdown ok");
    }
}

#[cfg(not(all(feature = "http", feature = "config")))]
#[test]
fn http_and_config_features_required() {
    // No-op: required features are not enabled
}
