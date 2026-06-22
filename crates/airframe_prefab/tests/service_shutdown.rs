#![forbid(unsafe_code)]

use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor};
use airframe_prefab::ServicePrefab;
use async_trait::async_trait;
use semver::Version;

/// A test module that runs a cancellable background loop and records ticks and drain flag.
struct BackgroundProbeModule {
    desc: ModuleDescriptor,
    ticks: Arc<AtomicUsize>,
    drained: Arc<AtomicBool>,
    cancel: Option<tokio_util::sync::CancellationToken>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl BackgroundProbeModule {
    fn new(ticks: Arc<AtomicUsize>, drained: Arc<AtomicBool>) -> Self {
        Self {
            desc: ModuleDescriptor {
                name: "background-probe",
                version: Version::parse("0.1.0").unwrap(),
                provides: &[],
                requires: &[],
                optional_requires: &[],
                requires_with_versions: &[],
                optional_requires_with_versions: &[],
            },
            ticks,
            drained,
            cancel: None,
            task: None,
        }
    }
}

#[async_trait]
impl Module for BackgroundProbeModule {
    fn descriptor(&self) -> &ModuleDescriptor {
        &self.desc
    }

    async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> {
        self.cancel = Some(ctx.cancel.clone());
        Ok(())
    }

    async fn start(&mut self) -> anyhow::Result<()> {
        let cancel = self.cancel.clone().expect("cancel present");
        let ticks = self.ticks.clone();
        let drained = self.drained.clone();
        self.task = Some(tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        drained.store(true, Ordering::SeqCst);
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {
                        ticks.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
        }));
        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        if let Some(t) = self.task.take() {
            let _ = t.await;
        }
        Ok(())
    }
}

#[tokio::test]
async fn service_prefab_graceful_shutdown_drains_tasks() {
    let ticks = Arc::new(AtomicUsize::new(0));
    let drained = Arc::new(AtomicBool::new(false));

    // Build service with the probe module
    let builder =
        ServicePrefab::new().with(BackgroundProbeModule::new(ticks.clone(), drained.clone()));

    let mut app = builder.start().await.expect("app starts");

    // Let it tick for a short while
    tokio::time::sleep(Duration::from_millis(150)).await;
    let before = ticks.load(Ordering::SeqCst);
    assert!(before > 0, "expected some ticks before shutdown");

    // Trigger cancellation and shutdown
    app.cancel.cancel();
    let shutdown_deadline = tokio::time::timeout(Duration::from_secs(2), app.shutdown()).await;
    assert!(
        shutdown_deadline.is_ok(),
        "shutdown should complete within deadline"
    );

    // After shutdown, task should have drained and no additional ticks should occur
    let after = ticks.load(Ordering::SeqCst);
    assert!(
        drained.load(Ordering::SeqCst),
        "background task should have observed cancellation and drained"
    );
    // Give a small window to ensure no more ticks are added post-shutdown
    tokio::time::sleep(Duration::from_millis(100)).await;
    let final_ticks = ticks.load(Ordering::SeqCst);
    assert_eq!(
        after, final_ticks,
        "ticks should not increase after shutdown"
    );
}
