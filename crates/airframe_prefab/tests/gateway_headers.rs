#![forbid(unsafe_code)]

// Integration tests for Gateway header policy and trace context behavior.
// Run with: cargo test -p airframe_prefab --features http,config --test gateway_headers -- --nocapture

#[cfg(all(feature = "http", feature = "config"))]
mod gateway_headers {
    use std::collections::HashMap;
    use std::net::SocketAddr;

    use airframe_core::app::AppBuilder;
    use airframe_http::axum_server::BoundAddr;
    use airframe_prefab::GatewayPrefab;
    use axum::{extract::Request, http::HeaderMap, routing::get, Json, Router};

    // Simple upstream server returning request headers as JSON map<String, String>.
    async fn start_upstream() -> (tokio::task::JoinHandle<()>, SocketAddr) {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/dump",
            get(|req: Request| async move {
                let mut map: HashMap<String, String> = HashMap::new();
                collect_headers(req.headers(), &mut map);
                Json(map)
            }),
        );
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (handle, addr)
    }

    fn collect_headers(h: &HeaderMap, out: &mut HashMap<String, String>) {
        for (k, v) in h.iter() {
            let key = k.as_str().to_string();
            let val = v.to_str().unwrap_or("");
            out.entry(key)
                .and_modify(|e| {
                    if e.is_empty() {
                        *e = val.to_string();
                    } else {
                        e.push_str(", ");
                        e.push_str(val);
                    }
                })
                .or_insert_with(|| val.to_string());
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn forwarded_headers_and_trace_context() {
        let (_up_task, up_addr) = start_upstream().await;

        // Configure gateway with single route to upstream
        let defaults: toml::Value = format!(
            "[server]\nbind = \"127.0.0.1:0\"\n\n[gateway]\nroutes = [\n  {{ path_prefix = \"/api\", upstream = \"http://{}\" }},\n]\n",
            up_addr
        )
        .parse()
        .unwrap();

        let builder: AppBuilder = GatewayPrefab::new()
            .with(airframe_config::ConfigModule::new(None).with_defaults(defaults));
        let app = builder.start().await.expect("gateway starts");
        let gw_addr = app.services.get::<BoundAddr>().expect("BoundAddr").0;

        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();

        // Case 1: No existing x-forwarded-for; provide X-Real-IP so gateway appends it.
        let url = format!("http://{}/api/dump", gw_addr);
        let resp = client
            .get(&url)
            .header("x-real-ip", "203.0.113.10")
            .send()
            .await
            .expect("resp");
        assert!(resp.status().is_success());
        let headers: HashMap<String, String> = resp.json().await.unwrap();
        // x-forwarded-for should exist and contain client IP
        let xff = headers.get("x-forwarded-for").cloned().unwrap_or_default();
        assert!(xff.contains("203.0.113.10"), "xff missing client ip: {xff}");
        // x-forwarded-proto and host should be set if absent on input
        assert_eq!(
            headers.get("x-forwarded-proto").map(String::as_str),
            Some("http")
        );
        assert!(headers.get("x-forwarded-host").is_some());
        // traceparent should exist and be valid if not provided
        let tp = headers.get("traceparent").cloned().unwrap_or_default();
        assert!(
            is_valid_traceparent(&tp),
            "generated traceparent invalid: {tp}"
        );

        // Case 2: Existing X-Forwarded-For must be appended, and trace headers preserved
        let resp2 = client
            .get(&url)
            .header("x-forwarded-for", "1.2.3.4")
            .header("x-real-ip", "5.6.7.8")
            .header(
                "traceparent",
                "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            )
            .header("tracestate", "rojo=00f067aa0ba902b7,congo=t61rcWkgMzE")
            .send()
            .await
            .expect("resp");
        assert!(resp2.status().is_success());
        let headers2: HashMap<String, String> = resp2.json().await.unwrap();
        let xff2 = headers2.get("x-forwarded-for").cloned().unwrap_or_default();
        assert!(xff2.contains("1.2.3.4"));
        assert!(
            xff2.ends_with("5.6.7.8"),
            "xff not appended correctly: {xff2}"
        );
        // Trace headers preserved
        assert_eq!(
            headers2.get("traceparent").map(String::as_str),
            Some("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01")
        );
        assert_eq!(
            headers2.get("tracestate").map(String::as_str),
            Some("rojo=00f067aa0ba902b7,congo=t61rcWkgMzE")
        );

        // Cleanup
        let mut app = app;
        app.shutdown().await.expect("shutdown ok");
    }

    fn is_valid_traceparent(tp: &str) -> bool {
        // Minimal W3C traceparent v00 validation: 00-<32hex>-<16hex>-<2hex>
        let parts: Vec<&str> = tp.split('-').collect();
        if parts.len() != 4 {
            return false;
        }
        if parts[0] != "00" {
            return false;
        }
        let trace_id = parts[1];
        let span_id = parts[2];
        let flags = parts[3];
        if trace_id.len() != 32 || span_id.len() != 16 || flags.len() != 2 {
            return false;
        }
        if !is_hex(trace_id) || !is_hex(span_id) || !is_hex(flags) {
            return false;
        }
        // Not all zeros
        if trace_id.chars().all(|c| c == '0') {
            return false;
        }
        if span_id.chars().all(|c| c == '0') {
            return false;
        }
        true
    }

    fn is_hex(s: &str) -> bool {
        s.chars().all(|c| c.is_ascii_hexdigit())
    }
}

#[cfg(not(all(feature = "http", feature = "config")))]
#[test]
fn features_required() {
    // No-op: required features are not enabled
}
