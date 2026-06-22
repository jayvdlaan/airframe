use std::sync::Arc;
use std::time::Duration;

use airframe_core::app::AppBuilder;
use airframe_health::{HealthModule, HealthService, HealthStatus};
use futures::FutureExt;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Build a minimal app with the HealthModule so HealthService is registered.
    let app = AppBuilder::new().with(HealthModule::new()).start().await?;
    let health = app.services.get::<HealthService>().expect("health svc");

    // Register a required check that becomes healthy after a short warmup.
    let flag = Arc::new(tokio::sync::Mutex::new(false));
    let flag2 = flag.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        *flag2.lock().await = true;
    });
    health.register_check("dummy", true, move |_cancel: CancellationToken| {
        let f = flag.clone();
        async move {
            if *f.lock().await {
                HealthStatus::Healthy
            } else {
                HealthStatus::Degraded("starting".into())
            }
        }
        .boxed()
    });

    // Register an optional check that always fails to illustrate status reporting.
    health.register_check("optional_background", false, |_c| {
        async move { HealthStatus::Unhealthy("disabled".into()) }.boxed()
    });

    // Wait for readiness (ignores optional failing check).
    health.ready().await;

    // Take a snapshot and render minimal JSON to stdout.
    let snapshot = health.checks_snapshot();
    #[derive(serde::Serialize)]
    struct CheckOut {
        name: String,
        required: bool,
        status: String,
    }

    let mut out: Vec<CheckOut> = Vec::new();
    for (name, required, f) in snapshot {
        let st = (f)(CancellationToken::new()).await; // run once for demo
        let status = match st {
            HealthStatus::Healthy => "healthy".to_string(),
            HealthStatus::Degraded(m) => format!("degraded: {}", m),
            HealthStatus::Unhealthy(m) => format!("unhealthy: {}", m),
        };
        out.push(CheckOut {
            name,
            required,
            status,
        });
    }
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}
