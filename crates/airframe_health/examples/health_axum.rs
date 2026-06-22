// cargo run -p airframe_health --example health_axum --features adapters-axum
// (legacy alias also works: --features http)
// Demonstrates mounting /readyz and /healthz via airframe_health on an Axum server.

use std::sync::Arc;
use std::time::Duration;

use airframe_health::{health_router, HealthStatus};
use airframe_http::axum_server::AxumServer;
use axum::Router;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Shared health state for HTTP routes
    let state = Arc::new(RwLock::new(HealthStatus::Unhealthy("starting".into())));

    // Build router with health routes and start server on an ephemeral port
    let app: Router = health_router(state.clone());
    let addr: std::net::SocketAddr = "127.0.0.1:8080".parse()?;
    let server = AxumServer::new(app, addr);

    // Spawn the server; it will print errors to stderr if any.
    tokio::spawn(async move {
        if let Err(e) = server.serve().await {
            eprintln!("server error: {e}");
        }
    });

    // Simulate initialization: transition to Healthy after a short delay
    tokio::time::sleep(Duration::from_millis(250)).await;
    {
        let mut g = state.write().await;
        *g = HealthStatus::Healthy;
    }

    println!("Health routes mounted. Try GET /readyz and /healthz on the bound address.");
    println!("Note: the bound port is ephemeral (0); check your server logs or integrate with AxumServerModule to retrieve the actual port.");

    // Run until Ctrl+C
    let _ = tokio::signal::ctrl_c().await;
    Ok(())
}
