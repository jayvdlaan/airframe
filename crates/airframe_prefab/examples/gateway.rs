// Minimal Gateway prefab example
// Run with:
//   cargo run -p airframe_prefab --features http --example gateway -- --config ./gateway.toml
// Example config (gateway.toml):
// [gateway]
// routes = [
//   { path_prefix = "/api", upstream = "http://127.0.0.1:9000" },
// ]

#![forbid(unsafe_code)]

#[cfg(feature = "http")]
use airframe_core::app::AppBuilder;
#[cfg(feature = "http")]
use airframe_prefab::GatewayPrefab;

#[cfg(feature = "http")]
#[tokio::main]
async fn main() {
    let builder: AppBuilder = GatewayPrefab::new();
    match builder.start().await {
        Ok(app) => {
            eprintln!("Gateway running. Press Ctrl+C to stop.");
            let _ = app.run_until_cancelled().await;
        }
        Err(e) => eprintln!("Gateway failed to start: {e}"),
    }
}

#[cfg(not(feature = "http"))]
fn main() {
    eprintln!("This example requires the 'http' feature.");
}
