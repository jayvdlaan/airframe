//! Airframe [`Module`] wiring: [`SchedulerModule`], plus the KV-driven
//! [`JobSpec`]/[`JobStrategy`] declarative job descriptors.

use std::{sync::Arc, time::Duration};

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_SCHEDULER};
use airframe_core::platform::PlatformSupport;
use airframe_kv::{kv_watch_prefix_t, KvEvent, KvStore, KvStoreExt};
use airframe_macros::module_descriptor;
use anyhow::Result;
use async_trait::async_trait;
use futures::{FutureExt, StreamExt};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::schedule::{Schedule, Strategy};
use crate::scheduler::{InMemoryScheduler, Scheduler};

pub struct SchedulerModule {
    desc: ModuleDescriptor,
}
impl Default for SchedulerModule {
    fn default() -> Self {
        Self::new()
    }
}

impl SchedulerModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "scheduler",
                version: "0.1.0",
                provides: [CAP_SCHEDULER.0]
            ),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum JobStrategy {
    Once { delay_ms: u64 },
    FixedRate { period_ms: u64 },
    FixedDelay { delay_ms: u64 },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct JobSpec {
    pub id: String,
    pub strategy: JobStrategy,
    pub max_runs: Option<u32>,
    pub timeout_ms: Option<u64>,
}

impl JobSpec {
    fn to_schedule(&self) -> Schedule {
        let strategy = match self.strategy {
            JobStrategy::Once { delay_ms } => Strategy::Once(Duration::from_millis(delay_ms)),
            JobStrategy::FixedRate { period_ms } => {
                Strategy::FixedRate(Duration::from_millis(period_ms))
            }
            JobStrategy::FixedDelay { delay_ms } => {
                Strategy::FixedDelay(Duration::from_millis(delay_ms))
            }
        };
        Schedule {
            strategy,
            max_runs: self.max_runs,
            timeout: self.timeout_ms.map(Duration::from_millis),
            retry: None,
            concurrency: None,
            jitter: None,
        }
    }
}

#[async_trait]
impl Module for SchedulerModule {
    airframe_macros::impl_descriptor!();

    fn platform_support(&self) -> PlatformSupport {
        PlatformSupport::desktop_only(
            "scheduler module is designed for long-running background timers/watchers and is not supported on mobile targets",
        )
    }

    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        info!(target = "airframe_scheduler", "scheduler initialized");
        let base = InMemoryScheduler::new();
        let sched = if let Some(bus) = ctx.services.event_bus() {
            Arc::new(base.with_event_bus(bus))
        } else {
            Arc::new(base)
        };
        ctx.services.register::<InMemoryScheduler>(sched.clone());
        // If KV is present, watch scheduler/jobs/* for JobSpec and (de)register jobs accordingly.
        if let Some(kv) = ctx.services.get::<dyn KvStore>() {
            // Subscribe for puts/updates first to minimize race windows
            let kv_puts = kv_watch_prefix_t::<JobSpec>(kv.clone(), "scheduler/jobs/")?;
            let puts_stream = kv_puts;
            let sched_clone = sched.clone();
            let kv_for_handler = kv.clone();
            // Handle puts/updates
            tokio::spawn(async move {
                let mut stream = puts_stream;
                while let Some((_key, spec, _meta)) = stream.next().await {
                    let id = spec.id.clone();
                    let _ = sched_clone.cancel_job(&id).await; // cancel if exists
                    let schedule = spec.to_schedule();
                    let kv_inner = kv_for_handler.clone();
                    let id_for_reg = id.clone();
                    // Handler will increment a counter at scheduler/jobs/<id>/ticks
                    let handler = Arc::new(move |_cancel: CancellationToken| {
                        let kv2 = kv_inner.clone();
                        let id2 = id.clone();
                        async move {
                            let ticks_key = format!("scheduler/jobs/{}/ticks", id2);
                            let current = if let Some((n, _)) =
                                KvStoreExt::get_t::<u64>(&*kv2, &ticks_key)
                                    .await
                                    .unwrap_or(None)
                            {
                                n
                            } else {
                                0
                            };
                            let _ = KvStoreExt::put_t(
                                &*kv2,
                                &ticks_key,
                                &(current + 1),
                                airframe_kv::PutOptions {
                                    ttl: None,
                                    if_match: None,
                                },
                            )
                            .await;
                            Ok(())
                        }
                        .boxed()
                    });
                    let _ = sched_clone
                        .register_job(&id_for_reg, schedule, handler)
                        .await;
                }
            });
            // Handle deletes
            let mut evts = kv.watch_prefix("scheduler/jobs/")?;
            let sched_clone2 = sched.clone();
            tokio::spawn(async move {
                while let Some(evt) = evts.next().await {
                    if let KvEvent::Delete { key } = evt {
                        if let Some(id) = key.strip_prefix("scheduler/jobs/") {
                            let _ = sched_clone2.cancel_job(id).await;
                        }
                    }
                }
            });

            // Startup reconciliation: list existing specs and register them immediately (synchronously in init).
            let mut existing_now = kv.list_prefix("scheduler/jobs/")?;
            while let Some((key, bytes, _meta)) = existing_now.next().await {
                if let Some(rest) = key.strip_prefix("scheduler/jobs/") {
                    if rest.contains('/') {
                        continue;
                    }
                    if let Ok(spec) = serde_json::from_slice::<JobSpec>(&bytes) {
                        let id = spec.id.clone();
                        let _ = sched.cancel_job(&id).await;
                        let schedule = spec.to_schedule();
                        let kv_inner = kv.clone();
                        let id_for_reg = id.clone();
                        let handler = Arc::new(move |_cancel: CancellationToken| {
                            let kv2 = kv_inner.clone();
                            let id2 = id.clone();
                            async move {
                                let ticks_key = format!("scheduler/jobs/{}/ticks", id2);
                                let current = if let Some((n, _)) =
                                    KvStoreExt::get_t::<u64>(&*kv2, &ticks_key)
                                        .await
                                        .unwrap_or(None)
                                {
                                    n
                                } else {
                                    0
                                };
                                let _ = KvStoreExt::put_t(
                                    &*kv2,
                                    &ticks_key,
                                    &(current + 1),
                                    airframe_kv::PutOptions {
                                        ttl: None,
                                        if_match: None,
                                    },
                                )
                                .await;
                                Ok(())
                            }
                            .boxed()
                        });
                        let _ = sched.register_job(&id_for_reg, schedule, handler).await;
                    }
                }
            }

            // Perform a second reconciliation shortly after startup to catch specs written right after init.
            let sched_clone4 = sched.clone();
            let kv_for_handler3 = kv.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(300)).await;
                if let Ok(mut again) = kv_for_handler3.list_prefix("scheduler/jobs/") {
                    while let Some((key, bytes, _meta)) = again.next().await {
                        if let Some(rest) = key.strip_prefix("scheduler/jobs/") {
                            if rest.contains('/') {
                                continue;
                            }
                            if let Ok(spec) = serde_json::from_slice::<JobSpec>(&bytes) {
                                let id = spec.id.clone();
                                let _ = sched_clone4.cancel_job(&id).await;
                                let schedule = spec.to_schedule();
                                let kv_inner = kv_for_handler3.clone();
                                let id_for_reg = id.clone();
                                let handler = Arc::new(move |_cancel: CancellationToken| {
                                    let kv2 = kv_inner.clone();
                                    let id2 = id.clone();
                                    async move {
                                        let ticks_key = format!("scheduler/jobs/{}/ticks", id2);
                                        let current = if let Some((n, _)) =
                                            KvStoreExt::get_t::<u64>(&*kv2, &ticks_key)
                                                .await
                                                .unwrap_or(None)
                                        {
                                            n
                                        } else {
                                            0
                                        };
                                        let _ = KvStoreExt::put_t(
                                            &*kv2,
                                            &ticks_key,
                                            &(current + 1),
                                            airframe_kv::PutOptions {
                                                ttl: None,
                                                if_match: None,
                                            },
                                        )
                                        .await;
                                        Ok(())
                                    }
                                    .boxed()
                                });
                                let _ = sched_clone4
                                    .register_job(&id_for_reg, schedule, handler)
                                    .await;
                            }
                        }
                    }
                }
            });
        }
        Ok(())
    }
}
