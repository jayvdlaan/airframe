// Minimal Worker prefab example with graceful drain and jittered retry
// Run:
//   cargo run -p airframe_prefab --example worker

#![forbid(unsafe_code)]

use airframe_prefab::worker::{RetryPolicy, WorkerModule};
use airframe_prefab::WorkerPrefab;
use tokio::sync::mpsc;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    // Build the app with Worker prefab and add a simple in-memory worker module
    let mut builder = WorkerPrefab::new();

    // Create a simple in-memory channel as our dev adapter/source
    let (tx, rx): (
        mpsc::UnboundedSender<Vec<u8>>,
        mpsc::UnboundedReceiver<Vec<u8>>,
    ) = WorkerModule::channel_pair();

    // Register a bytes handler with concurrency=2 and default retry policy
    let wm = WorkerModule::new().register_bytes_handler(
        "demo",
        2,
        rx,
        RetryPolicy::default(),
        |bytes| async move {
            let s = String::from_utf8(bytes).unwrap_or_else(|_| "<bin>".to_string());
            // Simulate some work; fail on a specific payload to demonstrate retry
            if s.contains("fail") {
                error!(target = "example::worker", msg = %s, "simulated failure");
                anyhow::bail!("simulated failure")
            } else {
                info!(target = "example::worker", msg = %s, "processed");
                Ok(())
            }
        },
    );

    builder = builder.with(wm);

    match builder.start().await {
        Ok(app) => {
            // Produce a few messages in the background
            let producer = {
                let tx = tx.clone();
                tokio::spawn(async move {
                    let msgs = ["hello", "world", "please fail", "done"];
                    for m in msgs {
                        let _ = tx.send(m.as_bytes().to_vec());
                    }
                })
            };

            // Run until Ctrl+C; then shutdown waits for workers
            let _ = app.run_until_cancelled().await;
            let _ = producer.await;
        }
        Err(e) => eprintln!("Worker failed to start: {e}"),
    }
}
