// cargo run -p airframe_http --example spec_e2e --features server,client --no-default-features

use std::net::SocketAddr;

use airframe_api::{http::Method, CodeSpec};
use airframe_http::{bytes::Bytes, reqwest_client::ReqwestClient, SpecClient};
use axum::{routing::get, Router};
use tokio::time::{sleep, Duration};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    // 1) Start a tiny Axum server on localhost
    let addr: SocketAddr = "127.0.0.1:18081".parse().unwrap();
    let app = Router::new().route("/hello/:name", get(hello));
    let server = airframe_http::axum_server::AxumServer::new(app, addr);
    tokio::spawn(async move {
        if let Err(e) = server.serve().await {
            eprintln!("server error: {e}");
        }
    });

    // Small delay to allow server to bind
    sleep(Duration::from_millis(100)).await;

    // 2) Build a CodeSpec describing the API we will call
    let base = "http://127.0.0.1:18081".parse().unwrap();
    let spec = CodeSpec::new(base).route("hello", Method::GET, "/hello/{name}");

    // 3) Compose SpecClient using a reqwest-backed HttpClient
    let client = ReqwestClient::new();
    let api = SpecClient::new(client, spec);

    // 4) Invoke the operation using params for path templating
    let params = serde_json::json!({ "name": "world" });
    let resp = api
        .invoke("hello", &params, Option::<Bytes>::None)
        .await
        .expect("request should succeed");
    assert!(
        resp.status().is_success(),
        "unexpected status: {}",
        resp.status()
    );
    let body = String::from_utf8(resp.body().to_vec()).unwrap();
    println!("Response: {}", body);

    Ok(())
}

async fn hello(axum::extract::Path(name): axum::extract::Path<String>) -> String {
    format!("Hello {}", name)
}
