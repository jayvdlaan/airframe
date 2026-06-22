//! server::module — the `AxumServerModule` Airframe Module integration.
//!
//! Wires the Axum server into the Airframe module lifecycle: resolves the bind
//! address, pre-binds the listener during `init`, composes the router from
//! registered contributors during `start`, and serves with graceful shutdown.
//! Extracted from `axum_server.rs` as a pure move; behavior is identical.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_HTTP_SERVER};
use airframe_core::platform::PlatformSupport;
use airframe_macros::module_descriptor;
use async_trait::async_trait;
use tracing::info;

use super::axum_server::BoundAddr;
use super::bind::resolve_bind_addr;

pub struct AxumServerModule {
    desc: ModuleDescriptor,
    bind: SocketAddr,
    #[allow(dead_code)]
    router: Option<Router>, // reserved for future use
    task: Option<JoinHandle<()>>,
    cancel_on_stop: bool,
    cancel: Option<tokio_util::sync::CancellationToken>,
    services: Option<airframe_core::registry::ServiceRegistry>,
    /// Pre-bound listener so BoundAddr is available during other modules' init.
    listener: Option<TcpListener>,
    #[cfg(feature = "config")]
    graceful: Option<std::time::Duration>,
}

impl AxumServerModule {
    pub fn new(bind: SocketAddr) -> Self {
        Self {
            desc: module_descriptor!(
                name: "http-axum-server",
                version: "0.1.0",
                provides: [CAP_HTTP_SERVER.0]
            ),
            bind,
            router: None,
            task: None,
            cancel_on_stop: true,
            cancel: None,
            services: None,
            listener: None,
            #[cfg(feature = "config")]
            graceful: None,
        }
    }
}

#[async_trait]
impl Module for AxumServerModule {
    airframe_macros::impl_descriptor!();

    fn platform_support(&self) -> PlatformSupport {
        PlatformSupport::desktop_only(
            "HTTP server modules are not supported on mobile targets (lifecycle/background/battery constraints)",
        )
    }

    async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> {
        // Always resolve bind before binding the listener, using provided bind as default
        let (resolved, source) = resolve_bind_addr(
            self.bind,
            #[cfg(feature = "config")]
            Some(&ctx),
            #[cfg(not(feature = "config"))]
            None,
        );
        self.bind = resolved;
        info!(target = "airframe_http", addr = %self.bind, source = %source, "AxumServerModule resolved bind");
        self.cancel = Some(ctx.cancel.clone());
        self.services = Some(ctx.services.clone());
        // Pre-bind the listener so BoundAddr is available during other modules' init
        let listener = TcpListener::bind(self.bind).await?;
        let actual_addr = listener.local_addr()?;
        self.listener = Some(listener);
        if let Some(svcs) = &self.services {
            svcs.register::<BoundAddr>(Arc::new(BoundAddr(actual_addr)));
        }
        Ok(())
    }

    async fn start(&mut self) -> anyhow::Result<()> {
        info!(target = "airframe_http", addr = %self.bind, "AxumServerModule start: binding and serving");

        // Build router from contributors now, after all modules have run init()
        // and registered. Borrow `self.services` (rather than cloning) so the
        // `registry_ptr` log and registry identity match the original behavior.
        let router = {
            let services = self.services.as_ref().expect("services set in init");

            // Initialize tracing once per-process if not already done by the host.
            Self::init_tracing_once(services);

            build_router(services)
        };

        // Use the pre-bound listener from init to ensure BoundAddr was available
        // for other modules.
        let listener = self
            .listener
            .take()
            .expect("listener should be bound in init");
        let cancel = self.cancel.clone().unwrap_or_default();
        self.task = Some(spawn_server(listener, router, cancel));
        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        info!(
            target = "airframe_http",
            "AxumServerModule stop: cancelling and awaiting server task"
        );
        if self.cancel_on_stop {
            if let Some(c) = &self.cancel {
                c.cancel();
            }
        }
        if let Some(h) = self.task.take() {
            let _ = h.await;
        }
        Ok(())
    }
}

impl AxumServerModule {
    /// Initialize a default env-filter based tracing subscriber once per process.
    /// If another global subscriber is already set, the error is ignored.
    fn init_tracing_once(services: &airframe_core::registry::ServiceRegistry) {
        struct TracingInitedToken;
        services.run_once::<TracingInitedToken, _>(|| {
            // Initialize a default env-filter based subscriber. If another global
            // subscriber is already set, ignore the error.
            let subscriber = tracing_subscriber::FmtSubscriber::builder()
                .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                .finish();
            let _ = tracing::subscriber::set_global_default(subscriber);
        });
    }
}

/// CORS-friendly preflight responder shared by the fallback health routes.
async fn health_options(req: axum::extract::Request) -> axum::response::Response {
    use axum::http::{HeaderValue, StatusCode};
    let mut res = axum::response::Response::builder().status(StatusCode::NO_CONTENT);
    if let Some(origin) = req.headers().get("Origin").and_then(|h| h.to_str().ok()) {
        let hv = HeaderValue::from_str(origin).unwrap_or(HeaderValue::from_static("*"));
        res = res.header("access-control-allow-origin", hv);
    }
    res = res.header(
        "access-control-allow-methods",
        HeaderValue::from_static("GET, OPTIONS"),
    );
    res.body(axum::body::Body::empty()).unwrap()
}

/// Build the base router with (optionally) fallback or contributed health routes.
fn build_health_base(services: &airframe_core::registry::ServiceRegistry) -> Router {
    use crate::server::router_contrib::get_or_create_health_registry;
    use axum::routing::{get, options};

    let health_reg = get_or_create_health_registry(services);

    let mut base = Router::new();

    // Allow additional health routes from registry. Only register the safety-net
    // /health and /healthz defaults when no health routes are contributed and when
    // explicitly enabled via env var AIRFRAME_HTTP_FALLBACK_HEALTH. This avoids
    // overlapping routes by default in app-managed health setups.
    let contributed = health_reg.get().is_some();
    let fallback_env = std::env::var("AIRFRAME_HTTP_FALLBACK_HEALTH").ok();
    let fallback_enabled = fallback_env
        .as_deref()
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    tracing::info!(
        target = "airframe_http",
        registry_ptr = ?(services as *const _),
        contributed = %contributed,
        fallback_enabled = %fallback_enabled,
        "server: health contributed? and fallback status"
    );
    let health_mount_opt = if contributed { health_reg.get() } else { None };
    if health_mount_opt.is_none() && fallback_enabled {
        // Safety net liveness for stacks expecting /health and /healthz
        base = base
            .route("/health", get(|| async { "ok" }))
            .route("/health", options(health_options))
            .route("/healthz", get(|| async { "healthy" }))
            .route("/healthz", options(health_options));
        tracing::info!(
            target = "airframe_http",
            "server: installed fallback /health and /healthz routes"
        );
    } else if health_mount_opt.is_none() {
        tracing::info!(
            target = "airframe_http",
            "server: no contributed health and fallback disabled; skipping fallback routes"
        );
    }
    if let Some(mount) = health_mount_opt {
        base = (mount)(base);
    }
    base
}

/// Mount all router contributors onto `base`, sorted deterministically by (phase, priority).
fn mount_contributors(services: &airframe_core::registry::ServiceRegistry, base: Router) -> Router {
    use crate::server::router_contrib::{get_or_create_contrib_registry, RouterContributor};

    let contrib_reg = get_or_create_contrib_registry(services);

    // Fetch all contributors and sort deterministically by (phase, priority)
    let mut ordered = contrib_reg.all();
    ordered.sort_by(|a, b| {
        let pa = a.phase();
        let pb = b.phase();
        if pa == pb {
            a.priority().cmp(&b.priority())
        } else {
            pa.cmp(&pb)
        }
    });
    let mut router: Router = base;
    for c in ordered.iter() {
        let c_ref: &dyn RouterContributor = c.as_ref();
        router = c_ref.mount(router);
    }
    router
}

/// Apply global layers, tracing middleware, and the error/response mapping middleware.
fn apply_layers(services: &airframe_core::registry::ServiceRegistry, router: Router) -> Router {
    use crate::server::router_contrib::{
        get_or_create_error_mapper_registry, get_or_create_layers_registry,
    };

    let layers_reg = get_or_create_layers_registry(services);
    let errmap_reg = get_or_create_error_mapper_registry(services);

    // Apply global HTTP layers now so they wrap the entire router (including contributed routes and health)
    let mut router = {
        let mut r = router;
        for applier in layers_reg.all().into_iter() {
            r = (applier)(r);
        }
        r
    };

    // Add request/response tracing middleware (TraceLayer) for visibility
    {
        use tower_http::trace::TraceLayer;
        router = router.layer(TraceLayer::new_for_http());
    }

    // Install an error/response mapping middleware at the end
    let mappers = errmap_reg.all();
    if !mappers.is_empty() {
        use axum::extract::Request as AxumRequest;
        use axum::middleware::from_fn;
        use axum::middleware::Next as AxumNext;
        use axum::response::Response as AxumResponse;
        let handler = move |req: AxumRequest, next: AxumNext| {
            let mappers = mappers.clone();
            async move {
                let mut resp: AxumResponse = next.run(req).await;
                for f in mappers.iter() {
                    resp = (f)(resp);
                }
                resp
            }
        };
        router = router.layer(from_fn(handler));
    }
    router
}

/// Compose the full router from the registered contributors, layers, and middleware.
fn build_router(services: &airframe_core::registry::ServiceRegistry) -> Router {
    use crate::server::router_contrib::get_or_create_metrics_registry;

    let metrics_reg = get_or_create_metrics_registry(services);

    // Run metrics hooks once on startup
    for hook in metrics_reg.all().into_iter() {
        (hook)(services);
    }

    let base = build_health_base(services);
    let router = mount_contributors(services, base);
    apply_layers(services, router)
}

/// Spawn the serve task with graceful shutdown driven by `cancel`.
fn spawn_server(
    listener: TcpListener,
    router: Router,
    cancel: tokio_util::sync::CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // Attach ConnectInfo<SocketAddr> so handlers can extract the
        // peer address (e.g. for IP capture on direct connections,
        // when no reverse proxy sets X-Forwarded-For). Use graceful
        // shutdown via the cancellation token.
        let server = axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move { cancel.cancelled().await });
        // Ignore the result to avoid propagating shutdown as an error
        let _ = server
            .await
            .map_err(|e| std::io::Error::other(format!("axum serve error: {e}")));
    })
}
