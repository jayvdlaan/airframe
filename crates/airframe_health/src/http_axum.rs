//! Axum route adapters for health probes, using airframe_http facilities when available.
//! This module is feature-gated and builds on the canonical behavior defined in
//! `crate::http` (paths and status-to-HTTP mapping).

#![cfg(any(feature = "adapters-axum", feature = "http"))]

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::http::{
    map_status_to_http_liveness, map_status_to_http_readiness, PATH_LIVENESS, PATH_READINESS,
};
use crate::HealthStatus;

use axum::{http::StatusCode, routing::get, Router};

/// Build a small Axum router that publishes readiness and liveness routes.
/// - Readiness: PATH_READINESS
/// - Liveness:  PATH_LIVENESS
///   Handlers use the canonical mapping helpers from `crate::http`.
pub fn health_router(state: Arc<RwLock<HealthStatus>>) -> Router {
    // Emit a trace that the health router is being constructed. This should appear
    // even in scenarios where route duplication would otherwise panic shortly after.
    tracing::info!(target = "airframe_health", state_ptr = ?Arc::as_ptr(&state), "health_router: building router");
    let ready_state = state.clone();
    let live_state_for_z = state.clone();
    let live_state_for_health = state.clone();
    Router::new()
        .route(
            PATH_READINESS,
            get(move || {
                let s = ready_state.clone();
                async move {
                    let st = s.read().await.clone();
                    let (code, body) = map_status_to_http_readiness(&st);
                    (StatusCode::from_u16(code).unwrap(), body)
                }
            }),
        )
        .route(
            PATH_LIVENESS,
            get(move || {
                let s = live_state_for_z.clone();
                async move {
                    let st = s.read().await.clone();
                    let (code, body) = map_status_to_http_liveness(&st);
                    (StatusCode::from_u16(code).unwrap(), body)
                }
            }),
        )
        // Provide the common alias /health for liveness, so both /health and /healthz
        // originate from airframe_health when mounted by applications. This avoids
        // duplicates with platform defaults when health routes are contributed.
        .route(
            "/health",
            get(move || {
                let s = live_state_for_health.clone();
                async move {
                    let st = s.read().await.clone();
                    let (code, body) = map_status_to_http_liveness(&st);
                    (StatusCode::from_u16(code).unwrap(), body)
                }
            }),
        )
}

/// Merge health routes into an existing router.
pub fn mount_health_routes(root: Router, state: Arc<RwLock<HealthStatus>>) -> Router {
    // Process-global guard to avoid double-merging health routes when helpers are
    // invoked from multiple places. This complements the register_health_probes
    // guard (which protects the contrib path) and ensures idempotency even if
    // mount_health_routes is called directly more than once.
    static HEALTH_MOUNTED_ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    tracing::info!(target = "airframe_health", state_ptr = ?Arc::as_ptr(&state), "health_router: mounting into existing router");
    if HEALTH_MOUNTED_ONCE.set(()).is_ok() {
        tracing::info!(
            target = "airframe_health",
            "health_router: first mount accepted"
        );
        root.merge(health_router(state))
    } else {
        tracing::warn!(
            target = "airframe_health",
            "health_router: mount skipped (already mounted globally)"
        );
        root
    }
}

/// RouterContributor adapter leveraging airframe_http's contributor seam.
// Deprecated: HealthProbeContributor moved to HealthContribRegistry usage.
// Kept adapter for now because it is used by docs/tests.
#[cfg(feature = "http")]
pub struct HealthProbeContributor {
    state: Arc<RwLock<HealthStatus>>,
}

#[cfg(feature = "http")]
impl HealthProbeContributor {
    pub fn new(state: Arc<RwLock<HealthStatus>>) -> Self {
        Self { state }
    }
}

#[cfg(feature = "http")]
impl airframe_http::axum_server::RouterContributor for HealthProbeContributor {
    fn mount(&self, router: Router) -> Router {
        router.merge(health_router(self.state.clone()))
    }
}

// Optional module ergonomics built on airframe_core ServiceRegistry and the contrib registry
#[cfg(feature = "http")]
pub struct HealthProbeState(pub Arc<RwLock<HealthStatus>>);

#[cfg(feature = "http")]
pub fn get_or_create_health_state(
    services: &airframe_core::registry::ServiceRegistry,
) -> Arc<RwLock<HealthStatus>> {
    if let Some(existing) = services.get::<HealthProbeState>() {
        existing.0.clone()
    } else {
        let state = Arc::new(RwLock::new(HealthStatus::Unhealthy("starting".into())));
        services.register::<HealthProbeState>(Arc::new(HealthProbeState(state.clone())));
        state
    }
}

#[cfg(feature = "http")]
pub fn register_health_probes(
    services: &airframe_core::registry::ServiceRegistry,
    state: Arc<RwLock<HealthStatus>>,
) {
    // Always log intent and the registry identity to aid diagnosing duplicate registries.
    tracing::info!(
        target = "airframe_health",
        registry_ptr = ?(services as *const _),
        "health: attempting to set mounter"
    );

    // Per-registry idempotency: only register once per ServiceRegistry.
    struct HealthRoutesRegisteredToken;
    services.run_once::<HealthRoutesRegisteredToken, _>(|| {
        let reg = airframe_http::axum_server::get_or_create_health_registry(services);
        // Try to set our health routes mounter. If another contributor already set one,
        // keep-first policy applies and this call will be ignored.
        match reg.set(Arc::new(move |router: axum::Router| {
            router.merge(health_router(state.clone()))
        })) {
            Ok(()) => tracing::info!(
                target = "airframe_health",
                registry_ptr = ?(services as *const _),
                "health: set accepted"
            ),
            Err(()) => tracing::warn!(
                target = "airframe_health",
                registry_ptr = ?(services as *const _),
                "health: set ignored (already set)"
            ),
        }
    });
}

#[cfg(all(test, feature = "http"))]
mod tests {
    use super::*;
    use airframe_http::axum_server::RouterContributor;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt; // for `oneshot` // bring trait into scope for contrib.mount()

    #[tokio::test]
    async fn readiness_and_liveness_handlers_return_expected_codes() {
        let state = Arc::new(RwLock::new(HealthStatus::Unhealthy("starting".into())));
        let app = health_router(state.clone());

        // starting -> readiness 503, liveness 503 (transitional)
        let req: Request<Body> = Request::builder()
            .uri(PATH_READINESS)
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::from_u16(503).unwrap());

        let req: Request<Body> = Request::builder()
            .uri(PATH_LIVENESS)
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::from_u16(503).unwrap());

        // healthy -> 200 on both
        {
            let mut w = state.write().await;
            *w = HealthStatus::Healthy;
        }
        let req: Request<Body> = Request::builder()
            .uri(PATH_READINESS)
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let req: Request<Body> = Request::builder()
            .uri(PATH_LIVENESS)
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // degraded -> 200
        {
            let mut w = state.write().await;
            *w = HealthStatus::Degraded("slow".into());
        }
        let req: Request<Body> = Request::builder()
            .uri(PATH_READINESS)
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // Unhealthy("boom") -> readiness 503, liveness 500
        {
            let mut w = state.write().await;
            *w = HealthStatus::Unhealthy("boom".into());
        }
        let req: Request<Body> = Request::builder()
            .uri(PATH_READINESS)
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::from_u16(503).unwrap());
        let req: Request<Body> = Request::builder()
            .uri(PATH_LIVENESS)
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::from_u16(500).unwrap());
    }

    #[tokio::test]
    async fn contributor_mounts_routes_equivalently() {
        let state = Arc::new(RwLock::new(HealthStatus::Unhealthy("starting".into())));
        let contrib = HealthProbeContributor::new(state.clone());
        let router = contrib.mount(Router::new());

        let req: Request<Body> = Request::builder()
            .uri(PATH_READINESS)
            .body(Body::empty())
            .unwrap();
        let res = router.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::from_u16(503).unwrap());
        let req: Request<Body> = Request::builder()
            .uri(PATH_LIVENESS)
            .body(Body::empty())
            .unwrap();
        let res = router.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::from_u16(503).unwrap());

        // Transition to healthy and assert 200s
        {
            let mut w = state.write().await;
            *w = HealthStatus::Healthy;
        }
        let req: Request<Body> = Request::builder()
            .uri(PATH_READINESS)
            .body(Body::empty())
            .unwrap();
        let res = router.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let req: Request<Body> = Request::builder()
            .uri(PATH_LIVENESS)
            .body(Body::empty())
            .unwrap();
        let res = router.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
