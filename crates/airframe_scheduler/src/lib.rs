//! Scheduler and job orchestration for Airframe apps.
//!
//! This crate is organized into focused modules:
//! - `schedule`: schedule description types ([`Strategy`], [`RetryPolicy`], [`Schedule`]).
//! - `scheduler`: the [`Scheduler`] trait and [`InMemoryScheduler`] implementation.
//! - `events`: job lifecycle events ([`JobStarted`], [`JobCompleted`], ...).
//! - `module`: Airframe [`SchedulerModule`] wiring plus [`JobSpec`]/[`JobStrategy`].
//! - `registry_ext`: [`ServiceRegistrySchedulerExt`] convenience accessor.

mod events;
mod module;
mod registry_ext;
mod schedule;
mod scheduler;
mod time;

pub use events::{JobCompleted, JobFailed, JobRetry, JobSkipped, JobStarted};
pub use module::{JobSpec, JobStrategy, SchedulerModule};
pub use registry_ext::ServiceRegistrySchedulerExt;
pub use schedule::{RetryPolicy, Schedule, Strategy};
pub use scheduler::{InMemoryScheduler, Scheduler};

#[cfg(test)]
mod tests {
    use super::*;
    use airframe_core::app::AppBuilder;
    use airframe_core::bus::EventBus;
    use anyhow::anyhow;
    use futures::{FutureExt, StreamExt};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn once_job_runs() {
        let app = AppBuilder::new()
            .with(SchedulerModule::new())
            .start()
            .await
            .unwrap();
        let sched = app
            .services
            .get::<InMemoryScheduler>()
            .expect("sched present");
        let counter = Arc::new(tokio::sync::Mutex::new(0u32));
        let ctr = counter.clone();
        sched
            .register_job(
                "once",
                Schedule {
                    strategy: Strategy::Once(Duration::from_millis(50)),
                    max_runs: None,
                    timeout: None,
                    retry: None,
                    concurrency: None,
                    jitter: None,
                },
                Arc::new(move |_cancel| {
                    let ctr = ctr.clone();
                    async move {
                        *ctr.lock().await += 1;
                        Ok(())
                    }
                    .boxed()
                }),
            )
            .await
            .unwrap();
        // Wait up to 500ms for the counter to increment
        let mut waited = 0u64;
        loop {
            if *counter.lock().await >= 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
            waited += 20;
            assert!(waited <= 500, "timeout waiting for once job");
        }
    }

    // Verify periodic timing deterministically using Tokio's test time.
    #[tokio::test(start_paused = true)]
    async fn periodic_fixed_rate_timing_with_test_clock() {
        let app = AppBuilder::new()
            .with(SchedulerModule::new())
            .start()
            .await
            .unwrap();
        let sched = app
            .services
            .get::<InMemoryScheduler>()
            .expect("sched present");
        let counter = Arc::new(tokio::sync::Mutex::new(0u32));
        let ctr = counter.clone();
        sched
            .register_job(
                "rate-testclock",
                Schedule {
                    strategy: Strategy::FixedRate(Duration::from_millis(100)),
                    max_runs: None,
                    timeout: None,
                    retry: None,
                    concurrency: None,
                    jitter: None,
                },
                Arc::new(move |_cancel| {
                    let ctr = ctr.clone();
                    async move {
                        *ctr.lock().await += 1;
                        Ok(())
                    }
                    .boxed()
                }),
            )
            .await
            .unwrap();

        // Initially zero
        assert_eq!(*counter.lock().await, 0);
        // Yield so the scheduler task can set up its first sleep
        tokio::task::yield_now().await;
        // interval tick fires immediately once
        assert_eq!(*counter.lock().await, 1);
        // Advance virtual time by exactly one period -> expect second run
        tokio::time::advance(Duration::from_millis(100)).await;
        // Allow the handler to run
        tokio::task::yield_now().await;
        assert_eq!(*counter.lock().await, 2);
        // Advance by one more period -> expect third run
        tokio::time::advance(Duration::from_millis(100)).await;
        tokio::task::yield_now().await;
        assert_eq!(*counter.lock().await, 3);
        // Advance by another period -> expect fourth run
        tokio::time::advance(Duration::from_millis(100)).await;
        tokio::task::yield_now().await;
        assert_eq!(*counter.lock().await, 4);
    }

    // Verify cancellation stops further executions.
    #[tokio::test(start_paused = true)]
    async fn cancellation_stops_repeating_job() {
        let app = AppBuilder::new()
            .with(SchedulerModule::new())
            .start()
            .await
            .unwrap();
        let sched = app
            .services
            .get::<InMemoryScheduler>()
            .expect("sched present");
        let counter = Arc::new(tokio::sync::Mutex::new(0u32));
        let ctr = counter.clone();
        sched
            .register_job(
                "cancel-me",
                Schedule {
                    strategy: Strategy::FixedRate(Duration::from_millis(50)),
                    max_runs: None,
                    timeout: None,
                    retry: None,
                    concurrency: None,
                    jitter: None,
                },
                Arc::new(move |_cancel| {
                    let ctr = ctr.clone();
                    async move {
                        *ctr.lock().await += 1;
                        Ok(())
                    }
                    .boxed()
                }),
            )
            .await
            .unwrap();

        // First tick happens immediately
        tokio::task::yield_now().await;
        assert_eq!(*counter.lock().await, 1);

        // Cancel and advance time further; counter should not increase
        sched.cancel_job("cancel-me").await.unwrap();
        tokio::time::advance(Duration::from_millis(500)).await;
        tokio::task::yield_now().await;
        assert_eq!(*counter.lock().await, 1);
    }

    #[tokio::test]
    async fn fixed_rate_runs_max_times() {
        // also validate KV-driven JobSpec registration
        let app = AppBuilder::new()
            .with(SchedulerModule::new())
            .start()
            .await
            .unwrap();
        let sched = app
            .services
            .get::<InMemoryScheduler>()
            .expect("sched present");
        let counter = Arc::new(tokio::sync::Mutex::new(0u32));
        let ctr = counter.clone();
        sched
            .register_job(
                "rate",
                Schedule {
                    strategy: Strategy::FixedRate(Duration::from_millis(10)),
                    max_runs: Some(3),
                    timeout: None,
                    retry: None,
                    concurrency: None,
                    jitter: None,
                },
                Arc::new(move |_cancel| {
                    let ctr = ctr.clone();
                    async move {
                        *ctr.lock().await += 1;
                        Ok(())
                    }
                    .boxed()
                }),
            )
            .await
            .unwrap();
        // Wait until it reaches 3
        let mut waited = 0u64;
        loop {
            let n = *counter.lock().await;
            if n >= 3 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
            waited += 20;
            assert!(waited <= 500, "timeout waiting for fixed_rate job, n={}", n);
        }
    }

    #[tokio::test]
    async fn publishes_job_started_and_completed_on_success() {
        let app = AppBuilder::new()
            .with(SchedulerModule::new())
            .start()
            .await
            .unwrap();
        // subscribe before registering
        let events = app.events.clone();
        let mut started = events.subscribe::<JobStarted>().unwrap();
        let mut completed = events.subscribe::<JobCompleted>().unwrap();
        let sched = app
            .services
            .get::<InMemoryScheduler>()
            .expect("sched present");
        sched
            .register_job(
                "evt_ok",
                Schedule {
                    strategy: Strategy::Once(Duration::from_millis(10)),
                    max_runs: None,
                    timeout: None,
                    retry: None,
                    concurrency: None,
                    jitter: None,
                },
                Arc::new(|_cancel| async move { Ok(()) }.boxed()),
            )
            .await
            .unwrap();
        // Expect Started then Completed for evt_ok
        let s = tokio::time::timeout(Duration::from_secs(1), started.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(s.id, "evt_ok");
        let c = tokio::time::timeout(Duration::from_secs(1), completed.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(c.id, "evt_ok");
    }

    #[tokio::test]
    async fn publishes_job_failed_on_error() {
        // no retry configured; expect immediate failure
        let app = AppBuilder::new()
            .with(SchedulerModule::new())
            .start()
            .await
            .unwrap();
        let events = app.events.clone();
        let mut started = events.subscribe::<JobStarted>().unwrap();
        let mut failed = events.subscribe::<JobFailed>().unwrap();
        let sched = app
            .services
            .get::<InMemoryScheduler>()
            .expect("sched present");
        sched
            .register_job(
                "evt_fail",
                Schedule {
                    strategy: Strategy::Once(Duration::from_millis(10)),
                    max_runs: None,
                    timeout: None,
                    retry: None,
                    concurrency: None,
                    jitter: None,
                },
                Arc::new(|_cancel| async move { Err(anyhow!("boom")) }.boxed()),
            )
            .await
            .unwrap();
        let s = tokio::time::timeout(Duration::from_secs(1), started.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(s.id, "evt_fail");
        let f = tokio::time::timeout(Duration::from_secs(1), failed.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(f.id, "evt_fail");
        assert!(f.error.contains("boom"));
    }

    #[tokio::test]
    async fn retries_then_succeeds() {
        let app = AppBuilder::new()
            .with(SchedulerModule::new())
            .start()
            .await
            .unwrap();
        let events = app.events.clone();
        let mut started = events.subscribe::<JobStarted>().unwrap();
        let mut retry_ev = events.subscribe::<JobRetry>().unwrap();
        let mut completed = events.subscribe::<JobCompleted>().unwrap();
        let sched = app
            .services
            .get::<InMemoryScheduler>()
            .expect("sched present");
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts2 = attempts.clone();
        sched
            .register_job(
                "retry_ok",
                Schedule {
                    strategy: Strategy::Once(Duration::from_millis(5)),
                    max_runs: None,
                    timeout: None,
                    retry: Some(RetryPolicy {
                        max_retries: 1,
                        backoff: Duration::from_millis(5),
                    }),
                    concurrency: None,
                    jitter: None,
                },
                Arc::new(move |_cancel| {
                    let attempts = attempts2.clone();
                    async move {
                        let a = attempts.fetch_add(1, Ordering::SeqCst);
                        if a == 0 {
                            Err(anyhow!("first fails"))
                        } else {
                            Ok(())
                        }
                    }
                    .boxed()
                }),
            )
            .await
            .unwrap();
        // Expect: Started, JobRetry(attempt=1), Started, Completed
        let _ = started.next().await.unwrap();
        let r = retry_ev.next().await.unwrap();
        assert_eq!(r.id, "retry_ok");
        assert_eq!(r.attempt, 1);
        let _ = started.next().await.unwrap();
        let c = completed.next().await.unwrap();
        assert_eq!(c.id, "retry_ok");
    }

    #[tokio::test]
    async fn no_overlap_under_fixed_rate() {
        let app = AppBuilder::new()
            .with(SchedulerModule::new())
            .start()
            .await
            .unwrap();
        let sched = app
            .services
            .get::<InMemoryScheduler>()
            .expect("sched present");
        let in_progress = Arc::new(AtomicBool::new(false));
        let violated = Arc::new(AtomicBool::new(false));
        let ip = in_progress.clone();
        let viol = violated.clone();
        sched
            .register_job(
                "no_overlap",
                Schedule {
                    strategy: Strategy::FixedRate(Duration::from_millis(5)),
                    max_runs: Some(3),
                    timeout: None,
                    retry: None,
                    concurrency: Some(1),
                    jitter: None,
                },
                Arc::new(move |_cancel| {
                    let ip = ip.clone();
                    let viol = viol.clone();
                    async move {
                        if ip.swap(true, Ordering::SeqCst) {
                            viol.store(true, Ordering::SeqCst);
                        }
                        tokio::time::sleep(Duration::from_millis(30)).await;
                        ip.store(false, Ordering::SeqCst);
                        Ok(())
                    }
                    .boxed()
                }),
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            !violated.load(Ordering::SeqCst),
            "job executions overlapped"
        );
    }

    #[tokio::test]
    async fn respects_concurrency_cap_no_overlap_even_with_short_period() {
        let app = AppBuilder::new()
            .with(SchedulerModule::new())
            .start()
            .await
            .unwrap();
        let sched = app
            .services
            .get::<InMemoryScheduler>()
            .expect("sched present");
        let in_progress = Arc::new(AtomicBool::new(false));
        let violated = Arc::new(AtomicBool::new(false));
        let ip = in_progress.clone();
        let viol = violated.clone();
        sched
            .register_job(
                "cap1",
                Schedule {
                    strategy: Strategy::FixedRate(Duration::from_millis(5)),
                    max_runs: Some(5),
                    timeout: None,
                    retry: None,
                    concurrency: Some(1),
                    jitter: None,
                },
                Arc::new(move |_cancel| {
                    let ip = ip.clone();
                    let viol = viol.clone();
                    async move {
                        if ip.swap(true, Ordering::SeqCst) {
                            viol.store(true, Ordering::SeqCst);
                        }
                        tokio::time::sleep(Duration::from_millis(30)).await;
                        ip.store(false, Ordering::SeqCst);
                        Ok(())
                    }
                    .boxed()
                }),
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!violated.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn concurrency_gt1_allows_parallel_runs() {
        let app = AppBuilder::new()
            .with(SchedulerModule::new())
            .start()
            .await
            .unwrap();
        let sched = app
            .services
            .get::<InMemoryScheduler>()
            .expect("sched present");
        let current = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));
        let cur2 = current.clone();
        let max2 = max_seen.clone();
        sched
            .register_job(
                "cap2",
                Schedule {
                    strategy: Strategy::FixedRate(Duration::from_millis(5)),
                    max_runs: Some(10),
                    timeout: None,
                    retry: None,
                    concurrency: Some(2),
                    jitter: None,
                },
                Arc::new(move |_cancel| {
                    let cur = cur2.clone();
                    let maxs = max2.clone();
                    async move {
                        let now = cur.fetch_add(1, Ordering::SeqCst) + 1;
                        loop {
                            let prev = maxs.load(Ordering::SeqCst);
                            if now > prev {
                                if maxs
                                    .compare_exchange(prev, now, Ordering::SeqCst, Ordering::SeqCst)
                                    .is_ok()
                                {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                        tokio::time::sleep(Duration::from_millis(40)).await;
                        cur.fetch_sub(1, Ordering::SeqCst);
                        Ok(())
                    }
                    .boxed()
                }),
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert_eq!(max_seen.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn jitter_applies_within_bounds() {
        let app = AppBuilder::new()
            .with(SchedulerModule::new())
            .start()
            .await
            .unwrap();
        let sched = app
            .services
            .get::<InMemoryScheduler>()
            .expect("sched present");
        let events = app.events.clone();
        let mut started = events.subscribe::<JobStarted>().unwrap();
        // Register a fast no-op job with jitter
        sched
            .register_job(
                "jit",
                Schedule {
                    strategy: Strategy::FixedRate(Duration::from_millis(20)),
                    max_runs: Some(5),
                    timeout: None,
                    retry: None,
                    concurrency: Some(1),
                    jitter: Some(Duration::from_millis(10)),
                },
                Arc::new(|_cancel| async move { Ok(()) }.boxed()),
            )
            .await
            .unwrap();
        // collect start times
        use std::time::Instant;
        let t0 = Instant::now();
        let mut times: Vec<u128> = Vec::new();
        for _ in 0..5 {
            let _evt = tokio::time::timeout(Duration::from_secs(1), started.next())
                .await
                .unwrap()
                .unwrap();
            times.push(t0.elapsed().as_millis());
        }
        // compute inter-run deltas
        let mut deltas: Vec<i128> = Vec::new();
        for w in times.windows(2) {
            deltas.push(w[1] as i128 - w[0] as i128);
        }
        // Each delta should be within [0 .. period + jitter] to tolerate timer jitter on CI
        for d in deltas {
            assert!((0..=35).contains(&d), "delta out of bounds: {}ms", d);
        }
    }
}

#[cfg(test)]
mod kv_spec_tests {
    use super::*;
    use airframe_core::app::AppBuilder;
    use airframe_kv::{KvModule, KvStore};
    use std::time::Duration;

    #[tokio::test]
    async fn kv_driven_job_spec_registers_and_runs_then_cancels() {
        // Build app with KV + Scheduler
        let app = AppBuilder::new()
            .with(KvModule::new())
            .with(SchedulerModule::new())
            .start()
            .await
            .unwrap();
        let kv = app
            .services
            .get::<airframe_kv::InMemoryKvStore>()
            .expect("inmem kv");
        // Subscribe BEFORE writing the JobSpec to avoid missing early events (not needed for this test)
        // Write a JobSpec at scheduler/jobs/ping
        let spec = JobSpec {
            id: "ping".into(),
            strategy: JobStrategy::FixedRate { period_ms: 10 },
            max_runs: Some(3),
            timeout_ms: None,
        };
        kv.put_t(
            "scheduler/jobs/ping",
            &spec,
            airframe_kv::PutOptions {
                ttl: None,
                if_match: None,
            },
        )
        .await
        .unwrap();
        // Wait for ticks to reach 3 using KV, which avoids nondeterminism in event delivery timing
        let mut waited = 0u64;
        loop {
            if let Some((n, _)) = kv.get_t::<u64>("scheduler/jobs/ping/ticks").await.unwrap() {
                if n >= 3 {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
            waited += 20;
            assert!(waited <= 5000, "timeout waiting for job ticks");
        }
        // Now delete spec to cancel
        let _ = kv.delete("scheduler/jobs/ping", None).await.unwrap();
        // Capture current count and ensure it doesn't increase further
        let (n0, _) = kv
            .get_t::<u64>("scheduler/jobs/ping/ticks")
            .await
            .unwrap()
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        let (n1, _) = kv
            .get_t::<u64>("scheduler/jobs/ping/ticks")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(n0, n1, "expected no further ticks after cancel");
    }
}
