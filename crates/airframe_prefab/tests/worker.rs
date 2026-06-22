#![forbid(unsafe_code)]

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, Mutex,
};

use airframe_core::app::AppBuilder;
use airframe_prefab::worker::{DlqSink, RetryPolicy, WorkerModule};
use async_trait::async_trait;

type DlqCallLog = Arc<Mutex<Vec<(String, Vec<u8>, u32, String)>>>;

struct TestDlq {
    pub calls: DlqCallLog,
}
#[async_trait]
impl DlqSink for TestDlq {
    async fn publish(
        &self,
        handler: &str,
        payload: &[u8],
        attempts: u32,
        error: &str,
    ) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push((
            handler.to_string(),
            payload.to_vec(),
            attempts,
            error.to_string(),
        ));
        Ok(())
    }
}

#[tokio::test]
async fn retry_until_success_invokes_handler_multiple_times() {
    let (tx, rx) = WorkerModule::channel_pair();
    let attempts = Arc::new(AtomicU32::new(0));
    let fail_until = 2u32; // fail 2 times, succeed on 3rd
    let a2 = attempts.clone();
    let worker = WorkerModule::new().register_bytes_handler(
        "h1",
        1,
        rx,
        RetryPolicy {
            max_attempts: 5,
            base_backoff_ms: 5,
            max_jitter_ms: 0,
        },
        move |_msg: Vec<u8>| {
            let a = a2.clone();
            async move {
                let n = a.fetch_add(1, Ordering::SeqCst) + 1;
                if n <= fail_until {
                    anyhow::bail!("fail {n}");
                }
                Ok(())
            }
        },
    );

    let app = AppBuilder::new().with(worker).start().await.expect("start");
    tx.send(b"x".to_vec()).unwrap();
    // Allow processing
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    // Expect at least 3 attempts
    assert!(attempts.load(Ordering::SeqCst) >= 3);

    // shutdown
    let mut app = app;
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn dlq_called_after_exhausting_retries() {
    let (tx, rx) = WorkerModule::channel_pair();
    let dlq_rec = Arc::new(Mutex::new(Vec::new()));
    let dlq = Arc::new(TestDlq {
        calls: dlq_rec.clone(),
    });

    let worker = WorkerModule::new().with_dlq(dlq).register_bytes_handler(
        "h2",
        1,
        rx,
        RetryPolicy {
            max_attempts: 2,
            base_backoff_ms: 5,
            max_jitter_ms: 0,
        },
        |_msg: Vec<u8>| async move { anyhow::bail!("always fail") },
    );

    let app = AppBuilder::new().with(worker).start().await.expect("start");
    tx.send(b"y".to_vec()).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let got = dlq_rec.lock().unwrap().clone();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, "h2");
    assert_eq!(got[0].2, 2); // attempts

    let mut app = app;
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn bounded_channel_and_max_inflight_limit_concurrency() {
    // Bounded channel with cap=1 and max_inflight=1 should ensure second message waits
    let (tx, rx) = WorkerModule::channel_pair_bounded(1);
    use tokio::sync::Notify;
    let gate = Arc::new(Notify::new());
    let concurrent = Arc::new(AtomicU32::new(0));
    let peak = Arc::new(AtomicU32::new(0));
    let ccur = concurrent.clone();
    let cpeak = peak.clone();
    let gate_in = gate.clone();
    let worker = WorkerModule::new().register_bytes_handler_bounded(
        "h3",
        2,
        rx,
        RetryPolicy {
            max_attempts: 1,
            base_backoff_ms: 0,
            max_jitter_ms: 0,
        },
        Some(1),
        move |_msg: Vec<u8>| {
            let c1 = ccur.clone();
            let p1 = cpeak.clone();
            let gate_local = gate_in.clone();
            async move {
                let now = c1.fetch_add(1, Ordering::SeqCst) + 1;
                // record peak
                let cur = p1.load(Ordering::SeqCst);
                if now > cur {
                    p1.compare_exchange(cur, now, Ordering::SeqCst, Ordering::SeqCst)
                        .ok();
                }
                // wait until released for first message, quick return otherwise
                if now == 1 {
                    gate_local.notified().await;
                }
                c1.fetch_sub(1, Ordering::SeqCst);
                Ok(())
            }
        },
    );

    let app = AppBuilder::new().with(worker).start().await.expect("start");
    // Send two messages
    tx.send(b"a".to_vec()).await.unwrap();
    // second send should not start processing (max_inflight=1)
    let _ = tx.try_send(b"b".to_vec()); // may err if full; fine for this test
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    // peak should be 1 so far
    assert_eq!(peak.load(Ordering::SeqCst), 1);
    // release
    gate.notify_one();
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    // still should never exceed 1 concurrently
    assert_eq!(peak.load(Ordering::SeqCst), 1);

    let mut app = app;
    app.shutdown().await.unwrap();
}
