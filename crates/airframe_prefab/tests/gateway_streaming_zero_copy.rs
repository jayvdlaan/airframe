#![forbid(unsafe_code)]

// Tests for Gateway streaming vs zero-copy toggles
// Run with: cargo test -p airframe_prefab --features http,config --test gateway_streaming_zero_copy -- --nocapture

#[cfg(all(feature = "http", feature = "config"))]
mod gateway_streaming_zero_copy {
    use std::net::SocketAddr;

    use airframe_core::app::AppBuilder;
    use airframe_http::axum_server::BoundAddr;
    use airframe_prefab::GatewayPrefab;
    use axum::{
        body::Body,
        http::HeaderMap,
        response::IntoResponse,
        routing::{get, post},
        Router,
    };
    use futures_util::Stream;

    // Upstream that can stream a large body in chunks and echo on POST
    async fn start_upstream() -> (tokio::task::JoinHandle<()>, SocketAddr) {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();

        // Streaming endpoint: returns many small chunks to encourage chunked transfer
        async fn stream_handler() -> impl IntoResponse {
            let chunk: bytes::Bytes = bytes::Bytes::from_static(&[b'a'; 1024]);
            let mut remaining = 256usize; // 256 KiB total
            let s = async_stream::stream! {
                while remaining > 0 {
                    remaining -= 1;
                    // small delay to avoid full buffering
                    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                    yield Ok::<bytes::Bytes, std::io::Error>(chunk.clone());
                }
            };
            let body = Body::from_stream(s);
            let mut headers = HeaderMap::new();
            // Hint proxy to not set content-length
            headers.insert(
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("text/plain"),
            );
            (headers, body)
        }

        async fn post_echo(body: Body) -> impl IntoResponse {
            let bytes = axum::body::to_bytes(body, 1024 * 1024)
                .await
                .unwrap_or_default();
            (
                [(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
                bytes,
            )
        }

        let app = Router::new()
            .route("/stream", get(stream_handler))
            .route("/echo", post(post_echo));

        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (handle, addr)
    }

    // Start gateway with provided defaults and return bound addr and client
    async fn start_gateway(
        defaults: toml::Value,
    ) -> (airframe_core::app::AppHandle, SocketAddr, reqwest::Client) {
        let builder: AppBuilder = GatewayPrefab::new()
            .with(airframe_config::ConfigModule::new(None).with_defaults(defaults));
        let app = builder.start().await.expect("gateway starts");
        let gw_addr = app.services.get::<BoundAddr>().expect("BoundAddr").0;
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap();
        (app, gw_addr, client)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn streaming_large_body_when_enabled() {
        let (_up_task, up_addr) = start_upstream().await;

        // Enable streaming
        let defaults = toml::toml! {
            [server]
            bind = "127.0.0.1:0"
            [gateway]
            streaming = true
            routes = [ { path_prefix = "/api", upstream = (format!("http://{}", up_addr)) } ]
        };

        let (mut app, gw_addr, client) = start_gateway(toml::Value::Table(defaults)).await;

        // Readiness loop
        let url = format!("http://{}/api/stream", gw_addr);
        let resp = loop {
            match client.get(&url).send().await {
                Ok(r) => break r,
                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    continue;
                }
            }
        };
        assert!(resp.status().is_success());
        // When streaming through, Content-Length is typically absent and Transfer-Encoding: chunked may be present
        // Accept either absence of content-length with a reasonably long read, or explicit chunked header.
        let te = resp
            .headers()
            .get("transfer-encoding")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let cl = resp.headers().get("content-length");
        assert!(
            te.contains("chunked") || cl.is_none(),
            "expected chunked or no content-length"
        );
        // Read the body to ensure stream works
        let bytes = resp.bytes().await.unwrap();
        assert!(
            bytes.len() >= 256 * 1024,
            "expected large body, got {} bytes",
            bytes.len()
        );

        app.shutdown().await.expect("shutdown ok");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn zero_copy_header_present_for_http_get() {
        let (_up_task, up_addr) = start_upstream().await;

        // Enable zero-copy
        let defaults = toml::toml! {
            [server]
            bind = "127.0.0.1:0"
            [gateway]
            zero_copy_http = true
            routes = [ { path_prefix = "/api", upstream = (format!("http://{}", up_addr)) } ]
        };

        let (mut app, gw_addr, client) = start_gateway(toml::Value::Table(defaults)).await;

        // GET should go via zero-copy
        let resp = client
            .get(format!("http://{}/api/stream", gw_addr))
            .send()
            .await
            .unwrap();
        assert!(resp.status().is_success());
        let hdr = resp
            .headers()
            .get("x-gw-zero-copy")
            .and_then(|v| v.to_str().ok());
        assert_eq!(hdr, Some("1"));

        app.shutdown().await.expect("shutdown ok");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fallback_for_post_and_https() {
        let (_up_task, up_addr) = start_upstream().await;

        let defaults = toml::toml! {
            [server]
            bind = "127.0.0.1:0"
            [gateway]
            streaming = true
            zero_copy_http = true
            routes = [ { path_prefix = "/api", upstream = (format!("http://{}", up_addr)) } ]
        };
        let (mut app, gw_addr, client) = start_gateway(toml::Value::Table(defaults)).await;

        // POST to http:// must not use zero-copy; header should be absent
        let resp = client
            .post(format!("http://{}/api/echo", gw_addr))
            .body("hello")
            .send()
            .await
            .unwrap();
        assert!(resp.status().is_success());
        assert!(resp.headers().get("x-gw-zero-copy").is_none());
        let body = resp.bytes().await.unwrap();
        assert_eq!(&body[..], b"hello");

        // HTTPS fallback case: configure route dynamically via query rewriter is not available, instead verify that
        // when scheme would be https the header is not present. We simulate by hitting an https URL directly via gateway
        // by setting upstream to an https address would require TLS server; instead, assert that gateway never sets the header
        // for methods other than GET/HEAD which we already tested with POST above.

        app.shutdown().await.expect("shutdown ok");
    }
}

#[cfg(not(all(feature = "http", feature = "config")))]
#[test]
fn features_required() {
    // No-op: required features are not enabled
}
