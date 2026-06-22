// See examples/basic_app.rs for rationale: this example is compiled only when
// the `with-external-examples` feature is enabled to avoid workspace cycles.
#[cfg(not(feature = "with-external-examples"))]
fn main() {
    eprintln!("Enable feature 'with-external-examples' to build this example: \
               cargo run -p airframe_core --example end_to_end_app --features with-external-examples");
}

#[cfg(feature = "with-external-examples")]
mod real_example {
    use airframe_core::bus::EventBus;
    use futures::FutureExt;
    use futures::StreamExt;
    use std::sync::Arc;
    use std::time::Duration;

    use airframe_args::ArgsModuleWithStartup;
    use airframe_config::ConfigModule;
    use airframe_core::app::AppBuilder;
    use airframe_health::{AppReady, HealthModule, HealthService, HealthStatus};
    use airframe_kv::{KvModule, KvStore, PutOptions};
    use airframe_logging::{LoggingChanged, LoggingModule};
    use airframe_scheduler::{JobSpec, JobStrategy, SchedulerModule};
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct Cli {}

    #[tokio::main]
    async fn main() -> anyhow::Result<()> {
        // Prepare a small temp config file to demonstrate layered config + hot reload
        let dir = tempfile::tempdir()?;
        let cfg_path = dir.path().join("app.toml");
        std::fs::write(&cfg_path, "[logging]\nlevel='warn'\n")?;

        // Build an app with Args, layered Config (from file), KV, Scheduler, Logging, and Health.
        let builder = AppBuilder::new()
            .with(ArgsModuleWithStartup::new())
            .with(ConfigModule::new(Some(cfg_path.clone())))
            .with(KvModule::new())
            .with(SchedulerModule::new())
            .with(LoggingModule::new())
            .with(HealthModule::new());

        // Print module graph DOT for docs
        let dot = builder.graph().to_dot();
        println!("Module graph (DOT):\n{}", dot);

        // Start the app
        let app = builder.start().await?;

        // Demonstrate: subscribe for events we want to observe
        let events = app.events.clone();
        let mut logging_changed = events.subscribe::<LoggingChanged>()?;
        let mut app_ready = events.subscribe::<AppReady>()?;

        // Register a delayed health check so we can observe AppReady
        if let Some(health) = app.services.get::<HealthService>() {
            let flag = Arc::new(tokio::sync::Mutex::new(false));
            let flag_set = flag.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                *flag_set.lock().await = true;
            });
            health.register_check("demo", true, move |_cancel| {
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

        // Demonstrate KV usage and Scheduler JobSpec from KV
        if let Some(kv) = app.services.get::<airframe_kv::InMemoryKvStore>() {
            // Write and read a KV key
            kv.put(
                "demo/hello",
                b"world",
                PutOptions {
                    ttl: Some(Duration::from_secs(1)),
                    if_match: None,
                },
            )
            .await?;
            if let Some((val, _meta)) = kv.get("demo/hello").await? {
                println!("KV demo/hello = {}", String::from_utf8_lossy(&val));
            }

            // Write a JobSpec that will increment a ticks counter (handled by SchedulerModule's KV integration)
            let spec = JobSpec {
                id: "heartbeat".into(),
                strategy: JobStrategy::FixedRate { period_ms: 50 },
                max_runs: Some(5),
                timeout_ms: None,
            };
            kv.put_t(
                "scheduler/jobs/heartbeat",
                &spec,
                PutOptions {
                    ttl: None,
                    if_match: None,
                },
            )
            .await?;
        }

        // Trigger a config change to demonstrate Logging reacting
        std::fs::write(&cfg_path, "[logging]\nlevel='info'\n")?;

        // Wait deterministically for events with short timeouts
        // 1) AppReady from Health
        if let Ok(Some(_)) = tokio::time::timeout(Duration::from_secs(2), app_ready.next()).await {
            println!("Health: AppReady received");
        }

        // 2) LoggingChanged after config reload
        if let Ok(Some(_)) =
            tokio::time::timeout(Duration::from_secs(2), logging_changed.next()).await
        {
            println!("Logging: config change observed");
        }

        // 3) Observe Scheduler ticks via KV
        if let Some(kv) = app.services.get::<airframe_kv::InMemoryKvStore>() {
            let mut waited = 0u64;
            loop {
                if let Some((n, _)) = kv.get_t::<u64>("scheduler/jobs/heartbeat/ticks").await? {
                    if n >= 5 {
                        println!("Scheduler heartbeat ran {} times", n);
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
                waited += 20;
                if waited > 3000 {
                    break;
                }
            }
        }

        // Clean shutdown shortly after
        app.cancel.cancel();
        Ok(())
    }
}
