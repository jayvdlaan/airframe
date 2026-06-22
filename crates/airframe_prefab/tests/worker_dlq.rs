#![forbid(unsafe_code)]

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use airframe_prefab::worker::{DlqSink, RetryPolicy, WorkerModule};
use airframe_prefab::WorkerPrefab;
use async_trait::async_trait;
use tokio::sync::mpsc;

struct TestDlq {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl DlqSink for TestDlq {
    async fn publish(
        &self,
        _handler: &str,
        _payload: &[u8],
        _attempts: u32,
        _error: &str,
    ) -> anyhow::Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

// Verifies that DLQ is invoked when a handler exhausts retries
#[tokio::test]
async fn worker_invokes_dlq_on_exhaust() {
    let calls = Arc::new(AtomicUsize::new(0));
    let dlq = Arc::new(TestDlq {
        calls: calls.clone(),
    });

    // Channel source
    let (tx, rx): (
        mpsc::UnboundedSender<Vec<u8>>,
        mpsc::UnboundedReceiver<Vec<u8>>,
    ) = WorkerModule::channel_pair();

    // Build worker that always fails, with small retry limits
    let worker = WorkerModule::new().with_dlq(dlq).register_bytes_handler(
        "failer",
        1,
        rx,
        RetryPolicy {
            max_attempts: 2,
            base_backoff_ms: 10,
            max_jitter_ms: 0,
        },
        |_bytes| async move { anyhow::bail!("always fail") },
    );

    let mut app = WorkerPrefab::new()
        .with(worker)
        .start()
        .await
        .expect("app starts");

    // Send a single message which will fail twice then DLQ
    tx.send(b"x".to_vec()).expect("send");

    // Wait for DLQ to be called
    let mut ok = false;
    for _ in 0..200 {
        // up to ~2s
        if calls.load(Ordering::SeqCst) > 0 {
            ok = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(ok, "DLQ was not invoked after retry exhaustion");

    // Shutdown
    app.cancel.cancel();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), app.shutdown()).await;
}
