# airframe_scheduler

Short description: In-memory scheduler service for the Airframe modular runtime.

## Overview

Schedules background jobs with multiple strategies (once, fixed-rate, fixed-delay), optional max runs, timeouts, retry policy, per-job concurrency caps, and jitter. Emits job lifecycle events on the EventBus. Optionally integrates with `airframe_kv` to register jobs from declarative `JobSpec` values under a KV prefix.

## Logical pieces

- Scheduler trait: async API to register/cancel jobs
- InMemoryScheduler: concrete implementation with per-job concurrency and cancellation tokens
- Strategy: `Once`, `FixedRate`, `FixedDelay`
- Schedule: wraps strategy + max_runs + timeout + retry + concurrency + jitter
- RetryPolicy: max_retries and backoff
- Job events: `JobStarted`, `JobCompleted`, `JobFailed`, `JobRetry`, `JobSkipped` (published on EventBus if present)
- KV integration: `JobSpec` + `JobStrategy` (when KV is available) — watches `scheduler/jobs/*` and registers/unregisters jobs
- ServiceRegistrySchedulerExt: helper to fetch the scheduler from the registry
- SchedulerModule: Airframe module that wires the service and optional KV integration

## Airframe module compatibility

- Compatibility: Yes — provides `cap:scheduler` via `SchedulerModule`
- Services: registers `InMemoryScheduler` into the ServiceRegistry
- Events: publishes job lifecycle events via the app EventBus

## Dependencies

- Rust dependencies: see Cargo.toml (tokio, futures, dashmap, tokio-util, serde, semver)
- System libraries: none
- Airframe capacities/modules: Exports `cap:scheduler`; optional KV-driven job management when `cap:kv` is available

## Setup / Installation

```toml
[dependencies]
airframe_core = { path = "../airframe_core" }
airframe_kv = { path = "../airframe_kv" } # optional, for KV-driven JobSpec
airframe_scheduler = { path = "../airframe_scheduler" }
```

## Usage

### Example 1: Wire the module, schedule a one-off job, and observe events

```rust
use std::sync::Arc;
use std::time::Duration;
use futures::FutureExt;
use futures::StreamExt;
use airframe_core::app::AppBuilder;
use airframe_core::bus::EventBus;
use airframe_scheduler::{SchedulerModule, InMemoryScheduler, Schedule, Strategy, JobStarted, JobCompleted, Scheduler};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new()
        .with(SchedulerModule::new())
        .start()
        .await?;

    // Subscribe to job lifecycle events
    let mut started = app.events.subscribe::<JobStarted>()?;
    let mut completed = app.events.subscribe::<JobCompleted>()?;

    // Register a job
    let sched = app.services.get::<InMemoryScheduler>().expect("scheduler");
    sched.register_job(
        "demo-once",
        Schedule { strategy: Strategy::Once(Duration::from_millis(50)), max_runs: None, timeout: None, retry: None, concurrency: None, jitter: None },
        Arc::new(|_cancel| async move { Ok(()) }.boxed()),
    ).await?;

    // Wait for signals
    let _ = tokio::time::timeout(Duration::from_secs(1), started.next()).await?;
    let _ = tokio::time::timeout(Duration::from_secs(1), completed.next()).await?;
    Ok(())
}
```

### Example 2: Drive jobs from KV JobSpec

```rust
use std::time::Duration;
use airframe_core::app::AppBuilder;
use airframe_kv::{KvModule, KvStoreExt, PutOptions};
use airframe_scheduler::{SchedulerModule, JobSpec, JobStrategy};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Wire KV + Scheduler; scheduler will watch `scheduler/jobs/*`
    let app = AppBuilder::new()
        .with(KvModule::new())
        .with(SchedulerModule::new())
        .start()
        .await?;

    // Write a JobSpec into KV
    let kv = app.services.get::<airframe_kv::InMemoryKvStore>().unwrap();
    let spec = JobSpec { id: "heartbeat".into(), strategy: JobStrategy::FixedRate { period_ms: 20 }, max_runs: Some(3), timeout_ms: None };
    KvStoreExt::put_t(&*kv, "scheduler/jobs/heartbeat", &spec, PutOptions { ttl: None, if_match: None }).await?;

    // Observe the scheduler increment a ticks counter per run
    tokio::time::sleep(Duration::from_millis(150)).await;
    let (ticks, _meta): (u64, _) = KvStoreExt::get_t(&*kv, "scheduler/jobs/heartbeat/ticks").await?.unwrap();
    assert!(ticks >= 3);
    Ok(())
}
```

## Examples and tests

- Run the basic once-run example: `cargo run -q -p airframe_scheduler --example scheduler_basic`
- Run the repeating job example: `cargo run -q -p airframe_scheduler --example repeating_job`
- Run tests: `cargo test -q -p airframe_scheduler`

## Status

Airframe module interface implemented (final step).

## License

This project is licensed under the repository license; see the top-level LICENSE file.
