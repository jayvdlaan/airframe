#![forbid(unsafe_code)]

// Integration test for OpenAPI placeholder endpoint.
// Run with features:
//   cargo test -p airframe_prefab --features http,openapi --test http_openapi -- --nocapture

#[cfg(all(feature = "http", feature = "openapi"))]
mod http_openapi {
    use airframe_core::app::AppBuilder;
    use airframe_http::axum_server::BoundAddr;
    use airframe_prefab::HttpApiServerPrefab;

    #[tokio::test]
    async fn openapi_endpoint_served_when_feature_enabled() {
        // Build from the HTTP API Server prefab (it conditionally wires OpenApiModule)
        let builder: AppBuilder = HttpApiServerPrefab::new();
        let app = builder.start().await.expect("app should start");

        // Discover the bound address
        let addr = app
            .services
            .get::<BoundAddr>()
            .expect("BoundAddr registered")
            .0;
        let url = format!("http://{}/openapi.json", addr);

        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("client build");

        let mut ok = false;
        let mut last_err: Option<anyhow::Error> = None;
        for _ in 0..50 {
            // ~5s readiness window
            match client.get(&url).send().await {
                Ok(resp) => {
                    assert!(
                        resp.status().is_success(),
                        "expected 2xx, got {:?}",
                        resp.status()
                    );
                    let body = resp.text().await.unwrap_or_default();
                    assert!(
                        body.contains("\"openapi\""),
                        "expected 'openapi' field in json: {}",
                        body
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
            panic!("failed to GET /openapi.json: {:?}", last_err);
        }

        let mut app = app;
        app.shutdown().await.expect("shutdown ok");
    }
}

#[cfg(not(all(feature = "http", feature = "openapi")))]
#[test]
fn features_required() {
    // No-op: required features are not enabled
}
