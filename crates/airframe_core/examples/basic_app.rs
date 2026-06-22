// This example depends on sibling crates (args, config, kv, scheduler, logging).
// To avoid workspace cycles during routine builds/tests, it is compiled only when
// the `with-external-examples` feature is enabled for airframe_core.

#[cfg(not(feature = "with-external-examples"))]
fn main() {
    eprintln!(
        "Enable feature 'with-external-examples' to build this example: \
               cargo run -p airframe_core --example basic_app --features with-external-examples"
    );
}

#[cfg(feature = "with-external-examples")]
mod real_example {
    use futures::FutureExt;
    use std::sync::Arc;
    use std::time::Duration;

    use airframe_args::ArgsModuleWithStartup;
    use airframe_config::ConfigModule;
    use airframe_core::app::AppBuilder;
    use airframe_kv::KvModule;
    use airframe_kv::KvStore;
    use airframe_logging::LoggingModule;
    use airframe_scheduler::{InMemoryScheduler, Schedule, Scheduler, SchedulerModule, Strategy};
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct Cli {}

    #[tokio::main]
    async fn main() -> anyhow::Result<()> {
        // Build an app with Args, Config, and KV modules.
        let app = AppBuilder::new()
            .with(ArgsModuleWithStartup::new())
            .with(ConfigModule::new(None))
            .with(KvModule::new())
            .with(SchedulerModule::new())
            .with(LoggingModule::new())
            .start()
            .await?;

        // Demonstrate using the KV store and waiting briefly before shutdown.
        if let Some(kv) = app.services.get::<airframe_kv::InMemoryKvStore>() {
            kv.put(
                "demo/hello",
                b"world",
                airframe_kv::PutOptions {
                    ttl: Some(Duration::from_secs(1)),
                    if_match: None,
                },
            )
            .await?;
        }

        // Optionally, schedule a one-off job via the Scheduler
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

        // Keep the app alive briefly then exit.
        app.cancel_after(Duration::from_millis(200));
        app.run_until_cancelled().await?;
        Ok(())
    }
}
