// Minimal Service prefab example
// Run with:
//   cargo run -p airframe_prefab --example service
// Or with HTTP admin (binds to 127.0.0.1:8080):
//   cargo run -p airframe_prefab --features http --example service

#![forbid(unsafe_code)]

use airframe_core::app::AppBuilder;
use airframe_prefab::ServicePrefab;
use tracing::info;

#[cfg(feature = "http")]
use airframe_http::admin::AdminModule;
#[cfg(feature = "http")]
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    // Build the app using the Service prefab and optionally attach HTTP admin.
    let builder: AppBuilder = ServicePrefab::new();
    #[cfg(feature = "http")]
    {
        let bind: SocketAddr = "127.0.0.1:8080".parse().expect("valid localhost address");
        let builder = builder
            .with(airframe_http::axum_server::AxumServerModule::new(bind))
            .with(AdminModule::new("admin", "0.1.0"));
        run_service(builder).await;
        return;
    }
    run_service(builder).await;
}

async fn run_service(builder: AppBuilder) {
    match builder.start().await {
        Ok(app) => {
            // Demonstrate a background task that respects cancellation for graceful shutdown.
            let cancel = app.cancel.clone();
            let worker = tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => {
                            // Drain/cleanup here
                            break;
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                            // Lightweight heartbeat log
                            info!(target = "example::service", "worker tick");
                        }
                    }
                }
            });

            // Wait for Ctrl+C, then request cancellation and perform graceful shutdown.
            let _ = tokio::signal::ctrl_c().await;
            // Signal background tasks via the shared cancellation token
            app.cancel.cancel();
            // Drain modules first; they may also observe cancellation
            let mut app = app; // get a mutable handle for shutdown
            let _ = app.shutdown().await;
            // Finally, wait for the worker to finish cleanup
            let _ = worker.await;
        }
        Err(e) => eprintln!("Service failed to start: {e}"),
    }
}
