//! Worker handler API and module for the Worker prefab.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_WORKER};
use airframe_core::platform::PlatformSupport;
use airframe_macros::module_descriptor;
use async_trait::async_trait;
use spacetime_core as st;
use tokio::sync::Mutex;
use tokio::sync::{mpsc, Semaphore};
use tracing::{debug, error, info, warn};

fn now_ms() -> u64 {
    let now = std::time::SystemTime::now();
    now.duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_millis(0))
        .as_millis() as u64
}

// Dead-letter queue sink interface for exhausted retries
#[async_trait]
pub trait DlqSink: Send + Sync {
    async fn publish(
        &self,
        handler: &str,
        payload: &[u8],
        attempts: u32,
        error: &str,
    ) -> anyhow::Result<()>;
}

#[derive(Clone, Copy, Debug)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_backoff_ms: u64, // exponential backoff base (100 => 100ms, 200ms, 400ms...)
    pub max_jitter_ms: u64,   // added jitter up to this many ms (deterministic per attempt)
}
impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_backoff_ms: 100,
            max_jitter_ms: 50,
        }
    }
}

impl RetryPolicy {
    #[cfg(feature = "config")]
    pub fn from_config(raw: &toml::Value) -> Self {
        let w = raw.get("worker");
        Self {
            max_attempts: w
                .and_then(|t| t.get("max_attempts"))
                .and_then(|v| v.as_integer())
                .unwrap_or(3) as u32,
            base_backoff_ms: w
                .and_then(|t| t.get("base_backoff_ms"))
                .and_then(|v| v.as_integer())
                .unwrap_or(100) as u64,
            max_jitter_ms: w
                .and_then(|t| t.get("max_jitter_ms"))
                .and_then(|v| v.as_integer())
                .unwrap_or(50) as u64,
        }
    }
}

type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

type HandlerFn = Arc<dyn Send + Sync + Fn(Vec<u8>) -> BoxFut<'static, anyhow::Result<()>>>;

type MetricsHookFn = Arc<dyn Fn(&str, &WorkerMetricsEvent) + Send + Sync>;

// Receiver kind: supports bounded and unbounded channels
enum ReceiverKind {
    Unbounded(Arc<Mutex<mpsc::UnboundedReceiver<Vec<u8>>>>),
    Bounded(Arc<Mutex<mpsc::Receiver<Vec<u8>>>>),
}

impl ReceiverKind {
    fn clone_ref(&self) -> Self {
        match self {
            ReceiverKind::Unbounded(r) => ReceiverKind::Unbounded(r.clone()),
            ReceiverKind::Bounded(r) => ReceiverKind::Bounded(r.clone()),
        }
    }

    async fn recv(&self) -> Option<Vec<u8>> {
        match self {
            ReceiverKind::Unbounded(r) => r.lock().await.recv().await,
            ReceiverKind::Bounded(r) => r.lock().await.recv().await,
        }
    }
}

struct HandlerSpec {
    name: &'static str,
    concurrency: usize,
    recv: ReceiverKind,
    handler: HandlerFn,
    retry: RetryPolicy,
    max_inflight: Option<Arc<Semaphore>>, // global across tasks for this handler
}

pub struct WorkerModule {
    desc: ModuleDescriptor,
    specs: Vec<HandlerSpec>,
    cancel: Option<tokio_util::sync::CancellationToken>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    dlq: Option<Arc<dyn DlqSink>>, // optional dead-letter sink
    #[cfg(feature = "config")]
    default_retry: RetryPolicy,
    services: Option<airframe_core::registry::ServiceRegistry>,
}

impl Default for WorkerModule {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkerModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "worker",
                version: "0.1.0",
                provides: [CAP_WORKER.0]
            ),
            specs: Vec::new(),
            cancel: None,
            tasks: Vec::new(),
            dlq: None,
            #[cfg(feature = "config")]
            default_retry: RetryPolicy::default(),
            services: None,
        }
    }

    /// Configure a dead-letter queue sink to receive messages that exhaust retries.
    pub fn with_dlq(mut self, dlq: Arc<dyn DlqSink>) -> Self {
        self.dlq = Some(dlq);
        self
    }

    pub fn register_bytes_handler<F, Fut>(
        mut self,
        name: &'static str,
        concurrency: usize,
        receiver: mpsc::UnboundedReceiver<Vec<u8>>,
        retry: RetryPolicy,
        handler: F,
    ) -> Self
    where
        F: Send + Sync + 'static + Fn(Vec<u8>) -> Fut,
        Fut: Send + 'static + Future<Output = anyhow::Result<()>>,
    {
        let recv = ReceiverKind::Unbounded(Arc::new(Mutex::new(receiver)));
        let handler: HandlerFn = Arc::new(move |msg| {
            let fut = handler(msg);
            Box::pin(fut)
        });
        self.specs.push(HandlerSpec {
            name,
            concurrency: concurrency.max(1),
            recv,
            handler,
            retry,
            max_inflight: None,
        });
        self
    }

    /// Register a bytes handler using the module's default RetryPolicy (from config when available).
    pub fn register_bytes_handler_default<F, Fut>(
        self,
        name: &'static str,
        concurrency: usize,
        receiver: mpsc::UnboundedReceiver<Vec<u8>>,
        handler: F,
    ) -> Self
    where
        F: Send + Sync + 'static + Fn(Vec<u8>) -> Fut,
        Fut: Send + 'static + Future<Output = anyhow::Result<()>>,
    {
        let retry = {
            #[cfg(feature = "config")]
            {
                self.default_retry.clone()
            }
            #[cfg(not(feature = "config"))]
            {
                RetryPolicy::default()
            }
        };
        self.register_bytes_handler(name, concurrency, receiver, retry, handler)
    }

    pub fn channel_pair() -> (
        mpsc::UnboundedSender<Vec<u8>>,
        mpsc::UnboundedReceiver<Vec<u8>>,
    ) {
        mpsc::unbounded_channel()
    }

    /// Bounded channel pair for backpressure. Sender.send() will await when full.
    pub fn channel_pair_bounded(
        capacity: usize,
    ) -> (mpsc::Sender<Vec<u8>>, mpsc::Receiver<Vec<u8>>) {
        mpsc::channel(capacity.max(1))
    }

    /// Register a handler using a bounded receiver and an optional max_inflight across all workers for this handler.
    pub fn register_bytes_handler_bounded<F, Fut>(
        mut self,
        name: &'static str,
        concurrency: usize,
        receiver: mpsc::Receiver<Vec<u8>>,
        retry: RetryPolicy,
        max_inflight: Option<usize>,
        handler: F,
    ) -> Self
    where
        F: Send + Sync + 'static + Fn(Vec<u8>) -> Fut,
        Fut: Send + 'static + Future<Output = anyhow::Result<()>>,
    {
        let recv = ReceiverKind::Bounded(Arc::new(Mutex::new(receiver)));
        let handler: HandlerFn = Arc::new(move |msg| {
            let fut = handler(msg);
            Box::pin(fut)
        });
        let sem = max_inflight.and_then(|n| {
            if n > 0 {
                Some(Arc::new(Semaphore::new(n)))
            } else {
                None
            }
        });
        self.specs.push(HandlerSpec {
            name,
            concurrency: concurrency.max(1),
            recv,
            handler,
            retry,
            max_inflight: sem,
        });
        self
    }
}

/// Compute exponential backoff delay in milliseconds with deterministic jitter.
fn compute_backoff_ms(
    retry: &RetryPolicy,
    attempt: u32,
    worker_index: usize,
    handler_name: &str,
) -> u64 {
    let shift: u32 = (attempt - 1).min(31);
    let factor: u64 = 1u64 << shift; // 1,2,4,8,... capped at 2^31
    let mut delay_ms = retry.base_backoff_ms.saturating_mul(factor);
    if retry.max_jitter_ms > 0 {
        // Deterministic jitter based on attempt, worker index, and handler name
        let mut seed: u64 = attempt as u64 ^ ((worker_index as u64) << 32);
        for b in handler_name.as_bytes() {
            seed = seed.wrapping_mul(1099511628211).wrapping_add(*b as u64);
        }
        let jitter = seed % (retry.max_jitter_ms + 1);
        delay_ms = delay_ms.saturating_add(jitter);
    }
    delay_ms
}

/// Run the pre-handle middleware chain, returning the (possibly modified) message.
/// Returns `Ok(working_msg)` if all middlewares pass, or `Err(e)` on first failure.
async fn run_middleware_chain(
    middlewares: &[Arc<dyn WorkerMiddleware>],
    name: &str,
    msg: &[u8],
) -> Result<Vec<u8>, anyhow::Error> {
    let mut working_msg = msg.to_vec();
    for m in middlewares.iter() {
        m.pre_handle(name, &mut working_msg).await?;
    }
    Ok(working_msg)
}

/// Emit a metrics event to all registered hooks.
fn emit_metrics(hooks: &[MetricsHookFn], name: &str, event: &WorkerMetricsEvent) {
    for hook in hooks.iter() {
        (hook)(name, event);
    }
}

/// Process a single message through the middleware chain with retry, DLQ, and metrics.
#[allow(clippy::too_many_arguments)]
async fn process_message(
    name: &str,
    worker_index: usize,
    msg: Vec<u8>,
    handler: &HandlerFn,
    retry: &RetryPolicy,
    middlewares: &[Arc<dyn WorkerMiddleware>],
    metrics_hooks: &[MetricsHookFn],
    dlq: &Option<Arc<dyn DlqSink>>,
    sem: &Option<Arc<Semaphore>>,
) {
    let _permit = match sem {
        Some(s) => Some(s.acquire().await.expect("semaphore closed")),
        None => None,
    };

    let start_ts = st::Instant::from_millis_since_epoch(now_ms());
    emit_metrics(metrics_hooks, name, &WorkerMetricsEvent::Received);

    let mut attempt: u32 = 0;
    loop {
        attempt += 1;

        let res = match run_middleware_chain(middlewares, name, &msg).await {
            Ok(working_msg) => {
                let res = handler(working_msg.clone()).await;
                for m in middlewares.iter() {
                    m.on_result(name, &working_msg, &res).await;
                }
                res
            }
            Err(e) => {
                error!(target = "airframe_worker", %name, worker = worker_index, attempt, error=%e, "pre_handle failed");
                emit_metrics(
                    metrics_hooks,
                    name,
                    &WorkerMetricsEvent::Failure {
                        attempts: attempt,
                        error: format!("pre: {}", e),
                    },
                );
                break;
            }
        };

        match res {
            Ok(_) => {
                debug!(target = "airframe_worker", %name, worker = worker_index, attempt, "message processed");
                let now = st::Instant::from_millis_since_epoch(now_ms());
                let dur = now.saturating_duration_since(start_ts).millis;
                emit_metrics(
                    metrics_hooks,
                    name,
                    &WorkerMetricsEvent::Success {
                        attempts: attempt,
                        latency_ms: dur,
                    },
                );
                break;
            }
            Err(e) => {
                if attempt >= retry.max_attempts {
                    error!(target = "airframe_worker", %name, worker = worker_index, attempt, error = %e, "message failed after max attempts");
                    if let Some(sink) = dlq {
                        let _ = sink.publish(name, &msg, attempt, &format!("{}", e)).await;
                    }
                    emit_metrics(
                        metrics_hooks,
                        name,
                        &WorkerMetricsEvent::Failure {
                            attempts: attempt,
                            error: format!("{}", e),
                        },
                    );
                    break;
                }
                let delay_ms = compute_backoff_ms(retry, attempt, worker_index, name);
                let st_delay = st::Duration::from_millis(delay_ms);
                warn!(target = "airframe_worker", %name, worker = worker_index, attempt, backoff_ms = st_delay.millis, error = %e, "handler error; retrying");
                emit_metrics(
                    metrics_hooks,
                    name,
                    &WorkerMetricsEvent::Retry {
                        attempt,
                        backoff_ms: st_delay.millis,
                    },
                );
                tokio::time::sleep(std::time::Duration::from_millis(st_delay.millis)).await;
            }
        }
    }
}

#[async_trait]
impl Module for WorkerModule {
    airframe_macros::impl_descriptor!();

    fn platform_support(&self) -> PlatformSupport {
        PlatformSupport::desktop_only(
            "worker module is designed for long-running background tasks and is not supported on mobile targets",
        )
    }

    async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> {
        self.cancel = Some(ctx.cancel.clone());
        self.services = Some(ctx.services.clone());
        #[cfg(feature = "config")]
        {
            if let Some(cfg) = ctx
                .services
                .get::<airframe_config::api::types::BasicConfig>()
            {
                self.default_retry = RetryPolicy::from_config(&cfg.raw);
            }
        }
        Ok(())
    }

    async fn start(&mut self) -> anyhow::Result<()> {
        let cancel = self.cancel.clone().unwrap_or_default();
        // Snapshot middleware and metrics hooks if registry exists
        let (middlewares, metrics_hooks) = if let Some(svcs) = &self.services {
            let m = get_or_create_worker_middleware_registry(svcs).all();
            let h = get_or_create_worker_metrics_registry(svcs).all();
            (m, h)
        } else {
            (vec![], vec![])
        };
        for spec in self.specs.iter() {
            let name = spec.name;
            for i in 0..spec.concurrency {
                let recv = spec.recv.clone_ref();
                let handler = Arc::clone(&spec.handler);
                let retry = spec.retry;
                let dlq = self.dlq.clone();
                let ctoken = cancel.clone();
                let sem = spec.max_inflight.clone();
                let middlewares = middlewares.clone();
                let metrics_hooks = metrics_hooks.clone();
                let handle = tokio::spawn(async move {
                    info!(target = "airframe_worker", %name, worker = i, "worker task started");
                    loop {
                        tokio::select! {
                            _ = ctoken.cancelled() => {
                                info!(target = "airframe_worker", %name, worker = i, "cancellation received; exiting worker");
                                break;
                            }
                            msg_opt = recv.recv() => {
                                let Some(msg) = msg_opt else {
                                    info!(target = "airframe_worker", %name, worker = i, "channel closed; exiting");
                                    break;
                                };
                                process_message(
                                    name, i, msg, &handler, &retry,
                                    &middlewares, &metrics_hooks, &dlq, &sem,
                                ).await;
                            }
                        }
                    }
                });
                self.tasks.push(handle);
            }
        }
        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        if let Some(c) = &self.cancel {
            c.cancel();
        }
        while let Some(h) = self.tasks.pop() {
            let _ = h.await;
        }
        Ok(())
    }
}

// --- Middleware and metrics registries ---

#[async_trait]
pub trait WorkerMiddleware: Send + Sync {
    async fn pre_handle(&self, _handler: &str, _msg: &mut Vec<u8>) -> anyhow::Result<()> {
        Ok(())
    }
    async fn on_result(&self, _handler: &str, _msg: &Vec<u8>, _res: &anyhow::Result<()>) {}
}

#[derive(Clone, Debug)]
pub enum WorkerMetricsEvent {
    Received,
    Success { attempts: u32, latency_ms: u64 },
    Failure { attempts: u32, error: String },
    Retry { attempt: u32, backoff_ms: u64 },
}

#[derive(Default)]
pub struct WorkerMiddlewareRegistry {
    inner: RwLock<Vec<Arc<dyn WorkerMiddleware>>>,
}
impl WorkerMiddlewareRegistry {
    pub fn add(&self, m: Arc<dyn WorkerMiddleware>) {
        self.inner.write().unwrap().push(m);
    }
    pub fn all(&self) -> Vec<Arc<dyn WorkerMiddleware>> {
        self.inner.read().unwrap().clone()
    }
}

#[derive(Default)]
pub struct WorkerMetricsHookRegistry {
    inner: RwLock<Vec<MetricsHookFn>>,
}
impl WorkerMetricsHookRegistry {
    pub fn add(&self, f: MetricsHookFn) {
        self.inner.write().unwrap().push(f);
    }
    pub fn all(&self) -> Vec<MetricsHookFn> {
        self.inner.read().unwrap().clone()
    }
}

pub fn get_or_create_worker_middleware_registry(
    svcs: &airframe_core::registry::ServiceRegistry,
) -> Arc<WorkerMiddlewareRegistry> {
    if let Some(r) = svcs.get::<WorkerMiddlewareRegistry>() {
        return r;
    }
    let reg = Arc::new(WorkerMiddlewareRegistry::default());
    svcs.register::<WorkerMiddlewareRegistry>(reg.clone());
    reg
}

pub fn get_or_create_worker_metrics_registry(
    svcs: &airframe_core::registry::ServiceRegistry,
) -> Arc<WorkerMetricsHookRegistry> {
    if let Some(r) = svcs.get::<WorkerMetricsHookRegistry>() {
        return r;
    }
    let reg = Arc::new(WorkerMetricsHookRegistry::default());
    svcs.register::<WorkerMetricsHookRegistry>(reg.clone());
    reg
}
