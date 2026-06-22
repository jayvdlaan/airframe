use airframe_core::app::AppBuilder;
use airframe_scheduler::{InMemoryScheduler, Schedule, Scheduler, SchedulerModule, Strategy};
use futures::FutureExt;
use std::sync::Arc;
use std::time::Duration;

// Run with:
// cargo run -q -p airframe_scheduler --example repeating_job
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new()
        .with(SchedulerModule::new())
        .start()
        .await?;

    let sched = app.services.get::<InMemoryScheduler>().expect("scheduler");
    let ticks = Arc::new(tokio::sync::Mutex::new(0u32));
    let ticks2 = ticks.clone();
    sched
        .register_job(
            "repeat-demo",
            Schedule {
                strategy: Strategy::FixedRate(Duration::from_millis(200)),
                max_runs: Some(5),
                timeout: None,
                retry: None,
                concurrency: None,
                jitter: None,
            },
            Arc::new(move |_cancel| {
                let t = ticks2.clone();
                async move {
                    let mut g = t.lock().await;
                    *g += 1;
                    println!("tick {}", *g);
                    Ok(())
                }
                .boxed()
            }),
        )
        .await?;

    // Wait until the demo ticks complete
    loop {
        if *ticks.lock().await >= 5 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    println!("repeating_job example finished");
    Ok(())
}
