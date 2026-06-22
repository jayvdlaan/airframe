#![forbid(unsafe_code)]

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use airframe_prefab::worker::{RetryPolicy, WorkerModule};
use airframe_prefab::WorkerPrefab;
use tokio::sync::mpsc;

// Verifies graceful drain: after cancellation, no new messages are pulled, while in-flight work completes.
#[tokio::test]
async fn worker_graceful_drain_on_cancel() {
    // Shared counter for processed messages
    let processed = Arc::new(AtomicUsize::new(0));

    // Channel source
    let (tx, rx): (
        mpsc::UnboundedSender<Vec<u8>>,
        mpsc::UnboundedReceiver<Vec<u8>>,
    ) = WorkerModule::channel_pair();

    // Handler that simulates some processing latency, then increments the counter
    let proc = processed.clone();
    let worker = WorkerModule::new().register_bytes_handler(
        "drain-test",
        2, // concurrency 2
        rx,
        RetryPolicy {
            max_attempts: 1,
            base_backoff_ms: 10,
            max_jitter_ms: 0,
        }, // no retries to keep timing tight
        move |_bytes| {
            let p = proc.clone();
            async move {
                // Simulate work
                tokio::time::sleep(Duration::from_millis(100)).await;
                p.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        },
    );

    let mut app = WorkerPrefab::new()
        .with(worker)
        .start()
        .await
        .expect("app starts");

    // Enqueue several messages
    for _ in 0..6 {
        tx.send(b"x".to_vec()).expect("send");
    }

    // Allow a very brief time for workers to pick up at most a couple of messages
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Trigger cancellation and wait for shutdown with a deadline
    app.cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), app.shutdown())
        .await
        .expect("shutdown timely");

    // After shutdown, processed should be > 0 and should not increase further after a brief wait
    let count = processed.load(Ordering::SeqCst);
    assert!(count > 0, "expected some in-flight work to complete");
    // Ensure stability: no new processing occurs after shutdown completes
    tokio::time::sleep(Duration::from_millis(100)).await;
    let after = processed.load(Ordering::SeqCst);
    assert_eq!(
        count, after,
        "no additional messages should be processed after shutdown"
    );
}
