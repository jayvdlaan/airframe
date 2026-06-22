use airframe_core::app::AppBuilder;
use airframe_core::bus::EventBus;
use airframe_health::{AppReady, HealthModule, HealthService, HealthStatus};
use futures::FutureExt;
use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new().with(HealthModule::new()).start().await?;

    // Subscribe before registering checks to avoid missed event
    let events = app.events.clone();
    let mut ready = events.subscribe::<AppReady>()?;

    if let Some(health) = app.services.get::<HealthService>() {
        let flag = Arc::new(tokio::sync::Mutex::new(false));
        let flag_set = flag.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            *flag_set.lock().await = true;
        });
        health.register_check("demo", true, move |_c| {
            let f = flag.clone();
            async move {
                if *f.lock().await {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Degraded("warming".into())
                }
            }
            .boxed()
        });
    }

    let _ = tokio::time::timeout(Duration::from_secs(2), ready.next()).await?;
    println!("AppReady received");

    app.cancel.cancel();
    Ok(())
}
