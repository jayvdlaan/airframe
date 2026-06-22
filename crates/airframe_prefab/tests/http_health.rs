#![forbid(unsafe_code)]

// Integration test for HTTP-prefab path. Requires `--features http` when running tests for this crate:
//   cargo test -p airframe_prefab --features http -- --nocapture

#[cfg(feature = "http")]
mod http_health {
    use airframe_core::app::AppBuilder;
    use airframe_http::axum_server::BoundAddr;
    use airframe_prefab::HttpApiServerPrefab;

    #[tokio::test]
    async fn http_prefab_serves_health() {
        // Build from the HTTP API Server prefab (binds to 127.0.0.1:8080 by default)
        let builder: AppBuilder = HttpApiServerPrefab::new();
        let app = builder.start().await.expect("app should start");

        // Discover the bound address from the service registry
        let addr = app
            .services
            .get::<BoundAddr>()
            .expect("BoundAddr should be registered")
            .0;

        // Build a client without proxy and with a small timeout to avoid hanging on misconfigured env proxies
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("client build");

        // Poll until the server is accepting connections (handle port-0 bind races)
        let url = format!("http://{}/health", addr);
        let mut last_err: Option<anyhow::Error> = None;
        let mut ok = false;
        for _ in 0..50 {
            // up to ~5s
            match client.get(&url).send().await {
                Ok(resp) => {
                    assert!(
                        resp.status().is_success(),
                        "expected 2xx, got {:?}",
                        resp.status()
                    );
                    ok = true;
                    break;
                }
                Err(e) => {
                    last_err = Some(anyhow::anyhow!(e));
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
        if !ok {
            panic!("failed to GET /health: {:?}", last_err);
        }

        // Shutdown
        let mut app = app; // take mutable to call shutdown
        app.shutdown().await.expect("shutdown ok");
    }
}

#[cfg(not(feature = "http"))]
#[test]
fn http_feature_required() {
    // No-op: required features are not enabled
}
