#![forbid(unsafe_code)]

// Integration test: toggles [cors.enable] and observes CORS headers without restart.
// Run with:
//   cargo test -p airframe_prefab --features http,config --test cors_hot_reload -- --nocapture

#[cfg(all(feature = "http", feature = "config"))]
mod cors_hot_reload {
    use airframe_core::bus::EventBus;
    use airframe_http::axum_server::BoundAddr;
    use airframe_prefab::HttpApiServerPrefab;
    use reqwest::header::HeaderMap;

    #[tokio::test]
    async fn toggles_cors_layer_without_restart() {
        // Build defaults with CORS disabled explicitly
        let defaults: toml::Value = toml::toml! {
            [server]
            bind = "127.0.0.1:0"
            [cors]
            enable = false
        }
        .into();

        // Start app
        let app = HttpApiServerPrefab::new()
            .with(airframe_config::ConfigModule::new(None).with_defaults(defaults))
            .start()
            .await
            .expect("app starts");
        let gw_addr = app.services.get::<BoundAddr>().expect("BoundAddr").0;

        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .unwrap();

        let origin = "http://example.com";
        // Initially, CORS disabled: expect no ACAO header on GET /health
        let url = format!("http://{}/health", gw_addr);
        let resp = client
            .request(reqwest::Method::GET, &url)
            .header("Origin", origin)
            .send()
            .await
            .unwrap();
        assert!(resp.status().is_success());
        let headers: HeaderMap = resp.headers().clone();
        assert!(
            headers.get("access-control-allow-origin").is_none(),
            "CORS should be disabled initially"
        );

        // Toggle: replace BasicConfig with cors.enable=true and publish ConfigReloaded
        {
            let cfg = app.services.get::<airframe_config::BasicConfig>().unwrap();
            let mut raw = cfg.raw.clone();
            // Set [cors.enable] = true
            if let Some(tbl) = raw.as_table_mut() {
                use toml::Value;
                let cors_tbl = tbl
                    .entry("cors".to_string())
                    .or_insert(Value::Table(toml::map::Map::new()));
                if let Some(t) = cors_tbl.as_table_mut() {
                    t.insert("enable".to_string(), Value::Boolean(true));
                }
            }
            let new_cfg = airframe_config::BasicConfig {
                raw,
                source: cfg.source.clone(),
            };
            app.services
                .register::<airframe_config::BasicConfig>(std::sync::Arc::new(new_cfg));
            if let Some(bus) = app.services.event_bus() {
                bus.publish(airframe_config::ConfigReloaded, None)
                    .await
                    .unwrap();
            }
        }

        // Wait briefly for hot-swap to apply
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Now expect ACAO header present on GET
        let resp2 = client
            .request(reqwest::Method::GET, &url)
            .header("Origin", origin)
            .send()
            .await
            .unwrap();
        assert!(resp2.status().is_success());
        let headers2: HeaderMap = resp2.headers().clone();
        assert!(
            headers2.get("access-control-allow-origin").is_some(),
            "CORS should be enabled after reload"
        );

        // Cleanup
        let mut app = app;
        app.shutdown().await.expect("shutdown ok");
    }
}

#[cfg(not(all(feature = "http", feature = "config")))]
#[test]
fn http_and_config_features_required() {
    // No-op: required features are not enabled
}
