use airframe_core::app::AppBuilder;
use airframe_core::bus::EventBus;
use airframe_scheduler::{
    InMemoryScheduler, JobCompleted, JobStarted, Schedule, Scheduler, SchedulerModule, Strategy,
};
use futures::FutureExt;
use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new()
        .with(SchedulerModule::new())
        .start()
        .await?;

    let events = app.events.clone();
    let mut started = events.subscribe::<JobStarted>()?;
    let mut completed = events.subscribe::<JobCompleted>()?;

    if let Some(sched) = app.services.get::<InMemoryScheduler>() {
        let _ = sched
            .register_job(
                "demo-once",
                Schedule {
                    strategy: Strategy::Once(Duration::from_millis(50)),
                    max_runs: None,
                    timeout: None,
                    retry: None,
                    concurrency: None,
                    jitter: None,
                },
                Arc::new(|_cancel| async move { Ok(()) }.boxed()),
            )
            .await;
    }

    let _ = tokio::time::timeout(Duration::from_secs(1), started.next()).await?;
    let _ = tokio::time::timeout(Duration::from_secs(1), completed.next()).await?;

    println!("scheduler demo-once run observed");
    app.cancel.cancel();
    Ok(())
}
