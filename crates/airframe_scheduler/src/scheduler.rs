//! The [`Scheduler`] trait and its in-memory implementation, [`InMemoryScheduler`].

use std::{sync::Arc, time::Duration};

use airframe_core::bus::EventBus;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use dashmap::DashMap;
use futures::future::BoxFuture;
use spacetime_core as st;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::events::{JobCompleted, JobFailed, JobRetry, JobSkipped, JobStarted};
use crate::schedule::{Schedule, Strategy};
use crate::time::{now_ms, sleep_with};

#[async_trait]
/// Scheduler service for registering and cancelling jobs.
/// - register_job: schedules execution according to Schedule; enforces per-job concurrency; emits Job events via EventBus if present.
/// - cancel_job: cancels future executions and in-flight runs via cancellation tokens.
pub trait Scheduler: Send + Sync {
    async fn register_job(
        &self,
        id: &str,
        schedule: Schedule,
        handler: Arc<dyn Fn(CancellationToken) -> BoxFuture<'static, Result<()>> + Send + Sync>,
    ) -> Result<()>;
    async fn cancel_job(&self, id: &str) -> Result<()>;
}

#[derive(Clone, Default)]
pub struct InMemoryScheduler {
    jobs: Arc<DashMap<String, (CancellationToken, JoinHandle<()>)>>,
    events: Option<Arc<airframe_core::bus::inmem::InMemoryEventBus>>,
    #[cfg(feature = "airframe-spacetime")]
    st_rt: Option<Arc<dyn st::Runtime + Send + Sync>>, // optional spacetime runtime for sleeping
}

impl InMemoryScheduler {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(DashMap::new()),
            events: None,
            #[cfg(feature = "airframe-spacetime")]
            st_rt: None,
        }
    }
    pub fn with_event_bus(mut self, bus: Arc<airframe_core::bus::inmem::InMemoryEventBus>) -> Self {
        self.events = Some(bus);
        self
    }

    /// When enabled, the scheduler will use the provided spacetime runtime's Timer to perform sleeps
    /// (wrapped in spawn_blocking to avoid blocking the async reactor). Default remains Tokio timers.
    #[cfg(feature = "airframe-spacetime")]
    pub fn with_spacetime_runtime(mut self, rt: Arc<dyn st::Runtime + Send + Sync>) -> Self {
        self.st_rt = Some(rt);
        self
    }
}

#[async_trait]
impl Scheduler for InMemoryScheduler {
    async fn register_job(
        &self,
        id: &str,
        schedule: Schedule,
        handler: Arc<dyn Fn(CancellationToken) -> BoxFuture<'static, Result<()>> + Send + Sync>,
    ) -> Result<()> {
        let id_s = id.to_string();
        let id_for_task = id_s.clone();
        let cancel = CancellationToken::new();
        let cancel_child = cancel.child_token();
        let events_bus = self.events.clone();
        // Capture optional spacetime runtime outside the task so we don't reference `self` inside the spawned future.
        #[cfg(feature = "airframe-spacetime")]
        let st_rt_captured: Option<Arc<dyn st::Runtime + Send + Sync>> = self.st_rt.clone();
        #[cfg(not(feature = "airframe-spacetime"))]
        let st_rt_captured: Option<Arc<dyn st::Runtime + Send + Sync>> = None;
        // Concurrency semaphore per job
        let permits = schedule.concurrency.unwrap_or(1).max(1) as usize;
        let semaphore = Arc::new(tokio::sync::Semaphore::new(permits));
        let handle = tokio::spawn(async move {
            let job_id = id_for_task;
            let mut runs: u32 = 0;
            // simple PRNG state seeded by job_id
            let mut rng_state: u64 = {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                job_id.hash(&mut h);
                h.finish()
            };
            fn next_u64(state: &mut u64) -> u64 {
                let mut z = state.wrapping_add(0x9E3779B97F4A7C15);
                *state = z;
                z ^= z >> 30;
                z = z.wrapping_mul(0xBF58476D1CE4E5B9);
                z ^= z >> 27;
                z = z.wrapping_mul(0x94D049BB133111EB);
                z ^ (z >> 31)
            }
            // jitter sleep helper inlined at call sites to avoid borrow across await
            match schedule.strategy {
                Strategy::Once(delay) => {
                    // pick runtime (if feature enabled and provided)
                    let rt_opt = st_rt_captured.clone();
                    tokio::select! {
                        _ = sleep_with(rt_opt.clone(), delay) => {},
                        _ = cancel_child.cancelled() => {
                            if let Some(bus) = &events_bus { let _ = bus.publish(JobSkipped { id: job_id.clone(), reason: "cancelled".into() }, None).await; }
                            return;
                        }
                    }
                    // jitter is not applied for Once per requirements
                    let mut attempt: u32 = 0;
                    loop {
                        if let Some(bus) = &events_bus {
                            let _ = bus.publish(JobStarted { id: job_id.clone() }, None).await;
                        }
                        let outcome = if let Some(t) = schedule.timeout {
                            match tokio::time::timeout(t, (handler)(cancel_child.clone())).await {
                                Ok(res) => res.map(|_| ()).map_err(|e| anyhow!(e.to_string())),
                                Err(_elapsed) => Err(anyhow!("timeout")),
                            }
                        } else {
                            (handler)(cancel_child.clone())
                                .await
                                .map(|_| ())
                                .map_err(|e| anyhow!(e.to_string()))
                        };
                        match outcome {
                            Ok(()) => {
                                if let Some(bus) = &events_bus {
                                    let _ = bus
                                        .publish(JobCompleted { id: job_id.clone() }, None)
                                        .await;
                                }
                                break;
                            }
                            Err(e) => {
                                if let Some(r) = &schedule.retry {
                                    if attempt < r.max_retries {
                                        attempt += 1;
                                        if let Some(bus) = &events_bus {
                                            let _ = bus
                                                .publish(
                                                    JobRetry {
                                                        id: job_id.clone(),
                                                        attempt,
                                                    },
                                                    None,
                                                )
                                                .await;
                                        }
                                        sleep_with(rt_opt.clone(), r.backoff).await;
                                        continue;
                                    }
                                }
                                if let Some(bus) = &events_bus {
                                    let _ = bus
                                        .publish(
                                            JobFailed {
                                                id: job_id.clone(),
                                                error: e.to_string(),
                                            },
                                            None,
                                        )
                                        .await;
                                }
                                break;
                            }
                        }
                    }
                }
                Strategy::FixedRate(period) => {
                    let mut interval = tokio::time::interval(period);
                    let rt_opt = st_rt_captured.clone();
                    loop {
                        tokio::select! {
                            _ = cancel_child.cancelled() => break,
                            _ = interval.tick() => {
                                if let Some(j) = schedule.jitter { let range = j.as_millis() as u64; let v = if range == 0 { 0 } else { next_u64(&mut rng_state) % (range + 1) }; sleep_with(rt_opt.clone(), Duration::from_millis(v)).await; }
                                // Try acquire permit; skip if at limit
                                match semaphore.clone().try_acquire_owned() {
                                    Ok(permit) => {
                                        // Start a run without awaiting completion
                                        let events_bus2 = events_bus.clone();
                                        let cancel_for_run = cancel_child.clone();
                                        let handler2 = handler.clone();
                                        let job_id2 = job_id.clone();
                                        let timeout = schedule.timeout;
                                        let retry = schedule.retry.clone();
                                        let run_rt = rt_opt.clone();
                                        tokio::spawn(async move {
                                            let mut attempt: u32 = 0;
                                            if let Some(bus) = &events_bus2 { let _ = bus.publish(JobStarted { id: job_id2.clone() }, None).await; }
                                            let res = async {
                                                loop {
                                                    let outcome = if let Some(t) = timeout {
                                                        match tokio::time::timeout(t, (handler2)(cancel_for_run.clone())).await {
                                                            Ok(res) => res.map(|_| ()).map_err(|e| anyhow!(e.to_string())),
                                                            Err(_elapsed) => Err(anyhow!("timeout")),
                                                        }
                                                    } else {
                                                        (handler2)(cancel_for_run.clone()).await.map(|_| ()).map_err(|e| anyhow!(e.to_string()))
                                                    };
                                                    match outcome {
                                                        Ok(()) => break Ok(()),
                                                        Err(e) => {
                                                            if let Some(r) = &retry { if attempt < r.max_retries { attempt += 1; if let Some(bus) = &events_bus2 { let _ = bus.publish(JobRetry { id: job_id2.clone(), attempt }, None).await; } sleep_with(run_rt.clone(), r.backoff).await; continue; } }
                                                            break Err(e);
                                                        }
                                                    }
                                                }
                                            }.await;
                                            match res {
                                                Ok(()) => { if let Some(bus) = &events_bus2 { let _ = bus.publish(JobCompleted { id: job_id2.clone() }, None).await; } },
                                                Err(e) => { if let Some(bus) = &events_bus2 { let _ = bus.publish(JobFailed { id: job_id2.clone(), error: e.to_string() }, None).await; } },
                                            }
                                            drop(permit);
                                        });
                                        runs += 1; if let Some(max) = schedule.max_runs { if runs >= max { break; } }
                                    },
                                    Err(_) => {
                                        if let Some(bus) = &events_bus { let _ = bus.publish(JobSkipped { id: job_id.clone(), reason: "concurrency".into() }, None).await; }
                                    }
                                }
                            }
                        }
                    }
                }
                Strategy::FixedDelay(delay) => {
                    let rt_opt = st_rt_captured.clone();
                    loop {
                        // Compute next wake deadline using spacetime_core Instant/Duration
                        let base = st::Instant::from_millis_since_epoch(now_ms());
                        let mut wake = base
                            .saturating_add(st::Duration::from_millis(delay.as_millis() as u64));
                        if let Some(j) = schedule.jitter {
                            let range = j.as_millis() as u64;
                            let v = if range == 0 {
                                0
                            } else {
                                next_u64(&mut rng_state) % (range + 1)
                            };
                            wake = wake.saturating_add(st::Duration::from_millis(v));
                        }
                        // Wait until computed deadline or cancellation
                        tokio::select! {
                            _ = cancel_child.cancelled() => break,
                            _ = async {
                                let now = st::Instant::from_millis_since_epoch(now_ms());
                                let rem = if wake >= now { wake.saturating_duration_since(now).millis } else { 0 };
                                if rem == 0 { return; }
                                sleep_with(rt_opt.clone(), Duration::from_millis(rem)).await
                            } => {}
                        }
                        match semaphore.clone().try_acquire_owned() {
                            Ok(permit) => {
                                let events_bus2 = events_bus.clone();
                                let cancel_for_run = cancel_child.clone();
                                let handler2 = handler.clone();
                                let job_id2 = job_id.clone();
                                let timeout = schedule.timeout;
                                let retry = schedule.retry.clone();
                                let run_rt = rt_opt.clone();
                                tokio::spawn(async move {
                                    let mut attempt: u32 = 0;
                                    if let Some(bus) = &events_bus2 {
                                        let _ = bus
                                            .publish(
                                                JobStarted {
                                                    id: job_id2.clone(),
                                                },
                                                None,
                                            )
                                            .await;
                                    }
                                    let res = async {
                                        loop {
                                            let outcome = if let Some(t) = timeout {
                                                match tokio::time::timeout(
                                                    t,
                                                    (handler2)(cancel_for_run.clone()),
                                                )
                                                .await
                                                {
                                                    Ok(res) => res
                                                        .map(|_| ())
                                                        .map_err(|e| anyhow!(e.to_string())),
                                                    Err(_elapsed) => Err(anyhow!("timeout")),
                                                }
                                            } else {
                                                (handler2)(cancel_for_run.clone())
                                                    .await
                                                    .map(|_| ())
                                                    .map_err(|e| anyhow!(e.to_string()))
                                            };
                                            match outcome {
                                                Ok(()) => break Ok(()),
                                                Err(e) => {
                                                    if let Some(r) = &retry {
                                                        if attempt < r.max_retries {
                                                            attempt += 1;
                                                            if let Some(bus) = &events_bus2 {
                                                                let _ = bus
                                                                    .publish(
                                                                        JobRetry {
                                                                            id: job_id2.clone(),
                                                                            attempt,
                                                                        },
                                                                        None,
                                                                    )
                                                                    .await;
                                                            }
                                                            sleep_with(run_rt.clone(), r.backoff)
                                                                .await;
                                                            continue;
                                                        }
                                                    }
                                                    break Err(e);
                                                }
                                            }
                                        }
                                    }
                                    .await;
                                    match res {
                                        Ok(()) => {
                                            if let Some(bus) = &events_bus2 {
                                                let _ = bus
                                                    .publish(
                                                        JobCompleted {
                                                            id: job_id2.clone(),
                                                        },
                                                        None,
                                                    )
                                                    .await;
                                            }
                                        }
                                        Err(e) => {
                                            if let Some(bus) = &events_bus2 {
                                                let _ = bus
                                                    .publish(
                                                        JobFailed {
                                                            id: job_id2.clone(),
                                                            error: e.to_string(),
                                                        },
                                                        None,
                                                    )
                                                    .await;
                                            }
                                        }
                                    }
                                    drop(permit);
                                });
                                runs += 1;
                                if let Some(max) = schedule.max_runs {
                                    if runs >= max {
                                        break;
                                    }
                                }
                            }
                            Err(_) => {
                                if let Some(bus) = &events_bus {
                                    let _ = bus
                                        .publish(
                                            JobSkipped {
                                                id: job_id.clone(),
                                                reason: "concurrency".into(),
                                            },
                                            None,
                                        )
                                        .await;
                                }
                            }
                        }
                    }
                }
            }
        });
        self.jobs.insert(id_s, (cancel, handle));
        Ok(())
    }

    async fn cancel_job(&self, id: &str) -> Result<()> {
        if let Some(entry) = self.jobs.remove(id) {
            let (cancel, handle) = entry.1;
            cancel.cancel();
            let _ = handle.await;
        }
        Ok(())
    }
}
