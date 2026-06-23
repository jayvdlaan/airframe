//! Health checks and readiness signaling for Airframe apps.
//!
//! `airframe_health` lets modules register required and optional health checks,
//! exposes a readiness barrier that resolves once all required checks are
//! healthy, and can publish an [`AppReady`] event when the app becomes ready.
//!
//! # Key pieces
//! - [`HealthService`] — register checks and query aggregated health.
//! - [`HealthStatus`] — a check's outcome (healthy / unhealthy).
//! - [`HealthModule`] — Airframe module providing `cap:health`.
//! - [`AppReady`] — event published when readiness is reached.
//! - [`ServiceRegistryHealthExt`] — convenience accessor on the registry.
//! - HTTP readiness/liveness probe routes are available under the `http` feature.
//!
//! # Example
//! ```ignore
//! use airframe_core::app::AppBuilder;
//! use airframe_health::HealthModule;
//!
//! # async fn run() -> anyhow::Result<()> {
//! let app = AppBuilder::new().with(HealthModule::new()).start().await?;
//! # Ok(()) }
//! ```
use std::sync::Arc;
use std::time::Duration;

use airframe_core::bus::{Event, EventBus};
use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_HEALTH};
use airframe_core::registry::ServiceRegistry;
use airframe_macros::module_descriptor;
use anyhow::Result;
use async_trait::async_trait;
use futures::{future::BoxFuture, FutureExt};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, trace};

pub mod http;
#[cfg(any(feature = "adapters-axum", feature = "http"))]
pub mod http_axum;

#[cfg(any(feature = "adapters-axum", feature = "http"))]
pub use http_axum::{
    get_or_create_health_state, health_router, mount_health_routes, register_health_probes,
    HealthProbeState,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded(String),
    Unhealthy(String),
}

pub type HealthCheckFn =
    Arc<dyn Fn(CancellationToken) -> BoxFuture<'static, HealthStatus> + Send + Sync + 'static>;

#[derive(Clone, Default)]
/// HealthService manages named health checks (required/optional) and provides utilities
/// like a readiness barrier (ready) that resolves when all required checks are Healthy.
pub struct HealthService {
    inner: Arc<dashmap::DashMap<String, (bool, HealthCheckFn)>>, // name -> (required, check)
}

impl HealthService {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(dashmap::DashMap::new()),
        }
    }
    pub fn register_check<F>(&self, name: &str, required: bool, f: F)
    where
        F: Fn(CancellationToken) -> BoxFuture<'static, HealthStatus> + Send + Sync + 'static,
    {
        self.inner
            .insert(name.to_string(), (required, Arc::new(move |c| f(c))));
    }

    pub fn checks_snapshot(&self) -> Vec<(String, bool, HealthCheckFn)> {
        self.inner
            .iter()
            .map(|e| (e.key().clone(), e.value().0, e.value().1.clone()))
            .collect()
    }

    /// A readiness barrier that resolves once all required checks return Healthy at the same time.
    /// It polls checks periodically; callers can cancel by wrapping with a timeout or external cancellation.
    pub async fn ready(&self) {
        // Mobile targets are more sensitive to wakeups. Keep semantics the same, but poll less aggressively.
        #[cfg(any(target_os = "android", target_os = "ios"))]
        let poll = Duration::from_millis(250);
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let poll = Duration::from_millis(25);
        loop {
            // snapshot and run concurrently
            let snapshot = self.checks_snapshot();
            if snapshot.is_empty() {
                // No required checks means trivially ready
                return;
            }
            let mut futs = Vec::with_capacity(snapshot.len());
            for (_name, required, f) in snapshot.iter() {
                let cancel = CancellationToken::new();
                let fut = (f)(cancel).map(|st| (*required, st));
                futs.push(fut);
            }
            let results = futures::future::join_all(futs).await;
            let mut all_required_healthy = true;
            for (required, status) in results {
                if required {
                    match status {
                        HealthStatus::Healthy => {}
                        _ => {
                            all_required_healthy = false;
                        }
                    }
                }
            }
            if all_required_healthy {
                return;
            }
            tokio::time::sleep(poll).await;
        }
    }
}

/// Compute an aggregate readiness status from a set of (required, status) pairs.
/// Rules:
/// - If any required check is Unhealthy, overall is Unhealthy (first reason wins).
/// - Else if any required check is Degraded, overall is Degraded (first reason wins).
/// - Else Healthy.
pub fn aggregate_readiness(results: &[(bool, HealthStatus)]) -> HealthStatus {
    for (required, st) in results {
        if *required {
            if let HealthStatus::Unhealthy(msg) = st {
                return HealthStatus::Unhealthy(msg.clone());
            }
        }
    }
    for (required, st) in results {
        if *required {
            if let HealthStatus::Degraded(msg) = st {
                return HealthStatus::Degraded(msg.clone());
            }
        }
    }
    HealthStatus::Healthy
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppReady;
impl Event for AppReady {
    const NAME: &'static str = "AppReady";
}

pub trait ServiceRegistryHealthExt {
    fn health(&self) -> Option<Arc<HealthService>>;
}
impl ServiceRegistryHealthExt for ServiceRegistry {
    fn health(&self) -> Option<Arc<HealthService>> {
        self.get::<HealthService>()
    }
}

pub struct HealthModule {
    desc: ModuleDescriptor,
    publish_once: bool,
    poll_interval: Duration,
}

impl Default for HealthModule {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "health",
                version: "0.1.0",
                provides: [CAP_HEALTH.0]
            ),
            publish_once: true,
            // Default polling interval for readiness evaluation.
            // On mobile targets (Android/iOS), avoid frequent wakeups by using a larger interval.
            #[cfg(any(target_os = "android", target_os = "ios"))]
            poll_interval: Duration::from_secs(1),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            poll_interval: Duration::from_millis(50),
        }
    }
    pub fn with_poll_interval(mut self, d: Duration) -> Self {
        self.poll_interval = d;
        self
    }
    pub fn with_publish_once(mut self, once: bool) -> Self {
        self.publish_once = once;
        self
    }
}

#[async_trait]
impl Module for HealthModule {
    airframe_macros::impl_descriptor!();

    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        info!(
            target = "airframe_health",
            publish_once = self.publish_once,
            poll_ms = self.poll_interval.as_millis(),
            "health module initialized"
        );
        let svc = Arc::new(HealthService::new());
        ctx.services.register::<HealthService>(svc.clone());

        // When the HTTP adapter feature is enabled, register health routes (e.g., /readyz, /healthz)
        // and keep the shared HealthStatus state updated from the evaluator loop.
        #[cfg(feature = "http")]
        let probe_state = {
            let state = get_or_create_health_state(&ctx.services);
            register_health_probes(&ctx.services, state.clone());
            Some(state)
        };

        #[cfg(not(feature = "http"))]
        let probe_state: Option<Arc<tokio::sync::RwLock<HealthStatus>>> = None;

        // evaluator task
        if let Some(bus) = ctx
            .services
            .get::<airframe_core::bus::inmem::InMemoryEventBus>()
        {
            let cancel = ctx.cancel.clone();
            let poll = self.poll_interval;
            let checks = svc.clone();
            let publish_once = self.publish_once;
            let probe_state = probe_state.clone();
            tokio::spawn(async move {
                let mut published = false;
                trace!(target = "airframe_health", "evaluator loop start");
                loop {
                    if cancel.is_cancelled() {
                        break;
                    }
                    let snapshot = checks.checks_snapshot();
                    // run all checks concurrently
                    let mut futs = Vec::with_capacity(snapshot.len());
                    for (_name, required, f) in snapshot.iter() {
                        let c = cancel.child_token();
                        let fut = (f)(c).map(|st| (*required, st));
                        futs.push(fut);
                    }
                    let results = futures::future::join_all(futs).await;
                    let mut all_required_healthy = true;
                    for (required, status) in results {
                        if required {
                            match status {
                                HealthStatus::Healthy => {}
                                _ => {
                                    all_required_healthy = false;
                                }
                            }
                        }
                    }

                    // Update the shared health probe state for HTTP endpoints.
                    if let Some(state) = probe_state.as_ref() {
                        let mut w = state.write().await;
                        *w = if all_required_healthy {
                            HealthStatus::Healthy
                        } else {
                            HealthStatus::Unhealthy("unready".into())
                        };
                    }

                    if all_required_healthy {
                        if !published || !publish_once {
                            debug!(target = "airframe_health", "publishing AppReady event");
                            let _ = bus.publish(AppReady, None).await;
                            if publish_once {
                                published = true;
                            }
                        }
                    } else {
                        // if not all healthy, allow future publish even if publish_once=false
                        if !publish_once {
                            published = false;
                        }
                    }
                    tokio::time::sleep(poll).await;
                }
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airframe_core::app::AppBuilder;
    use futures::StreamExt;

    #[tokio::test]
    async fn publishes_app_ready_after_delayed_check() {
        // Build app with HealthModule
        let app = AppBuilder::new()
            .with(HealthModule::new())
            .start()
            .await
            .unwrap();
        // Subscribe before registering a check to avoid missed events
        let events = app.events.clone();
        let mut ready = events.subscribe::<AppReady>().unwrap();
        let health = app.services.get::<HealthService>().expect("health svc");
        // Register a required check that becomes healthy after 100ms
        let flag = Arc::new(tokio::sync::Mutex::new(false));
        let flag2 = flag.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            *flag2.lock().await = true;
        });
        health.register_check("delayed", true, move |_cancel| {
            let flag = flag.clone();
            async move {
                if *flag.lock().await {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Degraded("warming".into())
                }
            }
            .boxed()
        });
        // Expect AppReady within 2s
        let _evt = tokio::time::timeout(Duration::from_secs(2), ready.next())
            .await
            .expect("no timeout")
            .expect("some event");
    }

    #[tokio::test]
    async fn ready_barrier_resolves_after_checks_healthy() {
        let app = AppBuilder::new()
            .with(HealthModule::new())
            .start()
            .await
            .unwrap();
        let health = app.services.get::<HealthService>().expect("health svc");
        // delayed healthy check
        let flag = Arc::new(tokio::sync::Mutex::new(false));
        let flag2 = flag.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(80)).await;
            *flag2.lock().await = true;
        });
        health.register_check("rdy", true, move |_cancel| {
            let flag = flag.clone();
            async move {
                if *flag.lock().await {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Unhealthy("no".into())
                }
            }
            .boxed()
        });
        // ready must resolve within 1s
        tokio::time::timeout(Duration::from_secs(1), health.ready())
            .await
            .expect("ready timed out");
    }

    #[tokio::test]
    async fn publish_once_configurable() {
        // Case 1: publish_once = true (default): only one AppReady even if flaps
        let app1 = AppBuilder::new()
            .with(HealthModule::new())
            .start()
            .await
            .unwrap();
        let events1 = app1.events.clone();
        let mut ready1 = events1.subscribe::<AppReady>().unwrap();
        let health1 = app1.services.get::<HealthService>().unwrap();
        let flag = Arc::new(tokio::sync::Mutex::new(false));
        let flag_set = flag.clone();
        health1.register_check("flap", true, move |_c| {
            let f = flag.clone();
            async move {
                if *f.lock().await {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Unhealthy("x".into())
                }
            }
            .boxed()
        });
        // become healthy -> expect first ready
        *flag_set.lock().await = true;
        let _ = tokio::time::timeout(Duration::from_secs(2), ready1.next())
            .await
            .unwrap()
            .unwrap();
        // flap unhealthy then healthy again
        *flag_set.lock().await = false;
        tokio::time::sleep(Duration::from_millis(120)).await;
        *flag_set.lock().await = true;
        // Should not receive a second AppReady within short time
        let second = tokio::time::timeout(Duration::from_millis(300), ready1.next()).await;
        assert!(
            second.is_err(),
            "unexpected second AppReady with publish_once=true"
        );

        // Case 2: publish_once = false: expect another AppReady after flap
        let app2 = AppBuilder::new()
            .with(HealthModule::new().with_publish_once(false))
            .start()
            .await
            .unwrap();
        let events2 = app2.events.clone();
        let mut ready2 = events2.subscribe::<AppReady>().unwrap();
        let health2 = app2.services.get::<HealthService>().unwrap();
        let flagb = Arc::new(tokio::sync::Mutex::new(false));
        let flagb_set = flagb.clone();
        health2.register_check("flap2", true, move |_c| {
            let f = flagb.clone();
            async move {
                if *f.lock().await {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Unhealthy("x".into())
                }
            }
            .boxed()
        });
        // First healthy
        *flagb_set.lock().await = true;
        let _ = tokio::time::timeout(Duration::from_secs(2), ready2.next())
            .await
            .unwrap()
            .unwrap();
        // Flap then healthy again -> expect second event
        *flagb_set.lock().await = false;
        tokio::time::sleep(Duration::from_millis(120)).await;
        *flagb_set.lock().await = true;
        let _again = tokio::time::timeout(Duration::from_secs(2), ready2.next())
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn optional_failing_does_not_block_ready() {
        let app = AppBuilder::new()
            .with(HealthModule::new())
            .start()
            .await
            .unwrap();
        let health = app.services.get::<HealthService>().unwrap();
        // Required healthy immediately
        health.register_check("r1", true, |_c| {
            async move { HealthStatus::Healthy }.boxed()
        });
        // Optional unhealthy
        health.register_check("o1", false, |_c| {
            async move { HealthStatus::Unhealthy("opt fail".into()) }.boxed()
        });
        // Ready should resolve quickly
        tokio::time::timeout(Duration::from_millis(200), health.ready())
            .await
            .expect("ready timed out");
    }

    #[test]
    fn aggregate_rules_work() {
        let r = vec![
            (true, HealthStatus::Healthy),
            (false, HealthStatus::Unhealthy("opt".into())),
        ];
        match aggregate_readiness(&r) {
            HealthStatus::Healthy => {}
            other => panic!("unexpected {other:?}"),
        }

        let r2 = vec![
            (true, HealthStatus::Degraded("warming".into())),
            (true, HealthStatus::Healthy),
        ];
        match aggregate_readiness(&r2) {
            HealthStatus::Degraded(msg) => assert_eq!(msg, "warming"),
            other => panic!("unexpected {other:?}"),
        }

        let r3 = vec![
            (true, HealthStatus::Healthy),
            (true, HealthStatus::Unhealthy("down".into())),
        ];
        match aggregate_readiness(&r3) {
            HealthStatus::Unhealthy(msg) => assert_eq!(msg, "down"),
            other => panic!("unexpected {other:?}"),
        }
    }
}
