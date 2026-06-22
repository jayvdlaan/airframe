#![forbid(unsafe_code)]

// Integration test for Gateway prefab minimal routing/proxy.
// Requires `--features "http,config"` because the routing table is built from `BasicConfig`.
//   cargo test -p airframe_prefab --features "http,config" --test gateway_routing -- --nocapture

#[cfg(all(feature = "http", feature = "config"))]
mod gateway_routing {
    use std::net::SocketAddr;

    use airframe_core::app::AppBuilder;
    use airframe_http::axum_server::BoundAddr;
    use airframe_prefab::GatewayPrefab;
    use axum::{routing::get, Router};

    async fn start_upstream() -> (tokio::task::JoinHandle<()>, SocketAddr) {
        // Bind upstream on localhost:0
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/echo", get(|| async { "upstream" }))
            .route("/v1/users", get(|| async { "users" }));
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (handle, addr)
    }

    #[tokio::test]
    async fn gateway_proxies_by_prefix() {
        // Start upstream server
        let (_handle, up_addr) = start_upstream().await;

        // Build an app from the prefab and inject gateway config via defaults rather than env
        let defaults: toml::Value = format!(
            "[gateway]\nroutes = [\n  {{ path_prefix = \"/api\", upstream = \"http://{}\" }},\n]\n",
            up_addr
        )
        .parse()
        .unwrap();

        // Start gateway from prefab with an extra ConfigModule that supplies defaults
        let builder: AppBuilder = GatewayPrefab::new()
            // ConfigModule is sufficient; ArgsModule is optional and not required for this test
            .with(airframe_config::ConfigModule::new(None).with_defaults(defaults));
        let app = builder.start().await.expect("gateway starts");

        // Discover gateway bound address
        let gw_addr = app.services.get::<BoundAddr>().expect("BoundAddr").0;

        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();

        // Readiness loop: GET /api/echo should return upstream body
        let url = format!("http://{}/api/echo", gw_addr);
        let mut ok = false;
        for _ in 0..50 {
            match client.get(&url).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let body = resp.text().await.unwrap_or_default();
                        assert_eq!(body, "upstream");
                        ok = true;
                        break;
                    }
                }
                Err(_) => {}
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        if !ok {
            panic!("gateway did not proxy within timeout");
        }

        // Cleanup
        let mut app = app;
        app.shutdown().await.expect("shutdown ok");
    }

    #[tokio::test]
    async fn routing_normalization_longest_prefix_and_health_precedence() {
        // Start three upstreams with distinguishable responses
        let (_h_a, up_a) = {
            let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
                .await
                .unwrap();
            let addr = listener.local_addr().unwrap();
            let app = Router::new()
                .route("/echo", get(|| async { "A" }))
                .route("/v1/users", get(|| async { "A-users" }));
            let h = tokio::spawn(async move {
                let _ = axum::serve(listener, app).await;
            });
            (h, addr)
        };
        let (_h_b, up_b) = {
            let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
                .await
                .unwrap();
            let addr = listener.local_addr().unwrap();
            let app = Router::new()
                .route("/", get(|| async { "B" }))
                .route("/echo", get(|| async { "B" }));
            let h = tokio::spawn(async move {
                let _ = axum::serve(listener, app).await;
            });
            (h, addr)
        };
        let (_h_c, up_c) = {
            let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
                .await
                .unwrap();
            let addr = listener.local_addr().unwrap();
            let app = Router::new()
                .route("/", get(|| async { "C" }))
                .route("/echo", get(|| async { "C" }));
            let h = tokio::spawn(async move {
                let _ = axum::serve(listener, app).await;
            });
            (h, addr)
        };

        // Build TOML manually to avoid escaping hassles (unsorted to exercise internal pre-sort)
        let defaults: toml::Value = toml::toml! {
            [logging]
            directives = ["info"]
            [server]
            bind = "127.0.0.1:0"
            [gateway]
            routes = [
                { path_prefix = "/", upstream = (format!("http://{}", up_c)) },
                { path_prefix = "/api/", upstream = (format!("http://{}", up_a)) },
                { path_prefix = "/api/admin/", upstream = (format!("http://{}", up_b)) },
            ]
        }
        .into();

        // Start gateway app
        let builder: AppBuilder = GatewayPrefab::new()
            .with(airframe_config::ConfigModule::new(None).with_defaults(defaults));
        let app = builder.start().await.expect("gateway starts");
        let gw_addr = app.services.get::<BoundAddr>().expect("BoundAddr").0;

        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();

        // Wait for readiness: use /readyz health endpoint mounted by HealthModule
        let mut ready = false;
        for _ in 0..50 {
            if let Ok(r) = client
                .get(format!("http://{}/readyz", gw_addr))
                .send()
                .await
            {
                if r.status().is_success() {
                    ready = true;
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        if !ready {
            panic!("server not ready in time");
        }

        // 1) Normalization: /api//v1/users -> collapses // and matches /api/ -> upstream A
        let resp = client
            .get(format!("http://{}/api//v1/users", gw_addr))
            .send()
            .await
            .unwrap();
        assert!(resp.status().is_success());
        assert_eq!(resp.text().await.unwrap(), "A-users");

        // 2) Longest-prefix: /api/admin (no trailing slash) should match /api/admin/ and go to B
        let resp = client
            .get(format!("http://{}/api/admin", gw_addr))
            .send()
            .await
            .unwrap();
        assert!(resp.status().is_success());
        assert_eq!(resp.text().await.unwrap(), "B");

        // 3) With trailing slash: /api/admin/ -> B (root)
        let resp = client
            .get(format!("http://{}/api/admin/", gw_addr))
            .send()
            .await
            .unwrap();
        assert!(resp.status().is_success());
        assert_eq!(resp.text().await.unwrap(), "B");

        // 4) /health should NOT be proxied. It is served by airframe_health as a liveness alias.
        let resp = client
            .get(format!("http://{}/health", gw_addr))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let body = resp.text().await.unwrap_or_default();
        assert_ne!(body, "C");

        // But /readyz should be served by health module (200)
        let resp = client
            .get(format!("http://{}/readyz", gw_addr))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);

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
