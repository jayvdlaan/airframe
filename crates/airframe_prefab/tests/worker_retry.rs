#![forbid(unsafe_code)]

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use airframe_core::app::AppBuilder;
use airframe_prefab::worker::{RetryPolicy, WorkerModule};
use airframe_prefab::WorkerPrefab;
use tokio::sync::mpsc;

// Verifies at-least-once semantics via retry: a handler that fails once then succeeds
#[tokio::test]
async fn worker_retries_and_eventually_succeeds() {
    // Shared counters for the handler
    let attempts = Arc::new(AtomicUsize::new(0));
    let successes = Arc::new(AtomicUsize::new(0));

    // Channel source
    let (tx, rx): (
        mpsc::UnboundedSender<Vec<u8>>,
        mpsc::UnboundedReceiver<Vec<u8>>,
    ) = WorkerModule::channel_pair();

    // Build app with worker module and a handler that fails first, then succeeds
    let a1 = attempts.clone();
    let s1 = successes.clone();
    let worker = WorkerModule::new().register_bytes_handler(
        "test",
        1,
        rx,
        RetryPolicy {
            max_attempts: 3,
            base_backoff_ms: 10,
            max_jitter_ms: 0,
        },
        move |_bytes| {
            let a1 = a1.clone();
            let s1 = s1.clone();
            async move {
                let n = a1.fetch_add(1, Ordering::SeqCst) + 1;
                if n == 1 {
                    // First attempt fails
                    anyhow::bail!("simulated failure on first attempt")
                } else {
                    s1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            }
        },
    );

    let builder: AppBuilder = WorkerPrefab::new().with(worker);
    let mut app = builder.start().await.expect("app starts");

    // Send a single message
    tx.send(b"msg".to_vec()).expect("send");

    // Wait until success observed (with timeout)
    let mut ok = false;
    for _ in 0..100 {
        // up to ~1s total
        if successes.load(Ordering::SeqCst) >= 1 {
            ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(ok, "handler did not eventually succeed after retry");

    // Clean shutdown
    app.cancel.cancel();
    let r = tokio::time::timeout(Duration::from_secs(2), app.shutdown()).await;
    assert!(r.is_ok(), "shutdown within deadline");

    // Ensure at least two attempts (failed once then succeeded)
    assert!(attempts.load(Ordering::SeqCst) >= 2);
}
