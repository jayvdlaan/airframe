//! server::router_contrib — helpers to compose Axum routers from multiple contributors.
//! Feature: `axum` must be enabled to use this module.

use std::sync::{Arc, RwLock};

use axum::Router;
use http::HeaderMap;

/// A pluggable seam to allow modules to contribute routes to an Axum router.
pub trait RouterContributor: Send + Sync {
    fn mount(&self, router: Router) -> Router;
}

/// Ordering phase for mounting router contributors. Variants sort in declaration order.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub enum RouterPhase {
    PreLayers,
    Routes,
    PostLayers,
    Fallback,
}

/// Extended contributor API that permits deterministic ordering via phase and priority.
/// Smaller priority mounts earlier within the same phase.
pub trait OrderedRouterContributor: RouterContributor + Send + Sync {
    fn phase(&self) -> RouterPhase {
        RouterPhase::Routes
    }
    fn priority(&self) -> i32 {
        0
    }
}

/// Apply all contributors to the given router in the provided order.
/// Note: This function does not re-order inputs. See AxumServerModule for ordered mounting.
pub fn mount_all<'a, I>(mut router: Router, contributors: I) -> Router
where
    I: IntoIterator<Item = &'a dyn RouterContributor>,
{
    for c in contributors {
        router = c.mount(router);
    }
    router
}

// ---------------------------------------------------------------------------
// Generic VecRegistry<T>
// ---------------------------------------------------------------------------

/// A thread-safe append-only registry backed by `Vec<Arc<T>>`.
/// Used to collect contributors, hooks, and policies from independent modules.
pub struct VecRegistry<T: ?Sized> {
    inner: RwLock<Vec<Arc<T>>>,
}

impl<T: ?Sized> Default for VecRegistry<T> {
    fn default() -> Self {
        Self {
            inner: RwLock::new(Vec::new()),
        }
    }
}

impl<T: ?Sized> VecRegistry<T> {
    pub fn add(&self, item: Arc<T>) {
        self.inner.write().unwrap().push(item);
    }
    pub fn all(&self) -> Vec<Arc<T>> {
        self.inner.read().unwrap().clone()
    }
}

/// Generic helper to get or create a `VecRegistry<T>` in the ServiceRegistry.
#[cfg(feature = "module")]
pub fn get_or_create_registry<T: ?Sized + Send + Sync + 'static>(
    svcs: &airframe_core::registry::ServiceRegistry,
) -> Arc<VecRegistry<T>> {
    if let Some(r) = svcs.get::<VecRegistry<T>>() {
        return r;
    }
    let reg = Arc::new(VecRegistry::default());
    svcs.register::<VecRegistry<T>>(reg.clone());
    reg
}

// ---------------------------------------------------------------------------
// Type aliases for concrete Vec registries
// ---------------------------------------------------------------------------

/// A registry capability storing all RouterContributor instances.
pub type RouterContribRegistry = VecRegistry<dyn OrderedRouterContributor>;

/// Global HTTP layer appliers. Each item takes a Router and returns a Router with a layer applied.
pub type GlobalLayerRegistry = VecRegistry<dyn Fn(axum::Router) -> axum::Router + Send + Sync>;

/// Error/response mappers applied to every response, in order.
pub type ErrorMapperRegistry =
    VecRegistry<dyn Fn(axum::response::Response) -> axum::response::Response + Send + Sync>;

/// Metrics hook registry. Hooks run during server start to register collectors/labels.
pub type MetricsHookRegistry =
    VecRegistry<dyn Fn(&airframe_core::registry::ServiceRegistry) + Send + Sync>;

/// Header policy hooks for the gateway proxy.
pub type GatewayHeaderPolicyRegistry = VecRegistry<dyn GatewayHeaderPolicy>;

/// Path/URL rewrite hooks for the gateway.
pub type GatewayRewriterRegistry = VecRegistry<dyn GatewayRewriter>;

// ---------------------------------------------------------------------------
// Backward-compatible helper functions (thin wrappers over get_or_create_registry)
// ---------------------------------------------------------------------------

/// Helper to get or create the RouterContribRegistry in the ServiceRegistry.
#[cfg(feature = "module")]
pub fn get_or_create_contrib_registry(
    svcs: &airframe_core::registry::ServiceRegistry,
) -> Arc<RouterContribRegistry> {
    get_or_create_registry(svcs)
}

/// Helper to get or create the GlobalLayerRegistry.
#[cfg(feature = "module")]
pub fn get_or_create_layers_registry(
    svcs: &airframe_core::registry::ServiceRegistry,
) -> Arc<GlobalLayerRegistry> {
    get_or_create_registry(svcs)
}

/// Helper to get or create the ErrorMapperRegistry.
#[cfg(feature = "module")]
pub fn get_or_create_error_mapper_registry(
    svcs: &airframe_core::registry::ServiceRegistry,
) -> Arc<ErrorMapperRegistry> {
    get_or_create_registry(svcs)
}

// ---------------------------------------------------------------------------
// HealthContribRegistry — kept as-is (single-slot, Option semantics)
// ---------------------------------------------------------------------------

/// Health check contributor registry for liveness/readiness routes.
/// Single-slot semantics: only one health router mounter may be registered.
#[derive(Default)]
pub struct HealthContribRegistry {
    #[allow(clippy::type_complexity)]
    inner: RwLock<Option<Arc<dyn Fn(axum::Router) -> axum::Router + Send + Sync>>>,
}
impl HealthContribRegistry {
    /// Set the health routes mounter. Returns Err(()) if one is already set (keep-first policy).
    #[allow(clippy::result_unit_err)]
    pub fn set(
        &self,
        mounter: Arc<dyn Fn(axum::Router) -> axum::Router + Send + Sync>,
    ) -> Result<(), ()> {
        let mut guard = self.inner.write().unwrap();
        if guard.is_some() {
            return Err(());
        }
        *guard = Some(mounter);
        Ok(())
    }
    /// Get the currently set mounter, if any.
    pub fn get(&self) -> Option<Arc<dyn Fn(axum::Router) -> axum::Router + Send + Sync>> {
        self.inner.read().unwrap().clone()
    }
}

#[cfg(feature = "module")]
pub fn get_or_create_health_registry(
    svcs: &airframe_core::registry::ServiceRegistry,
) -> Arc<HealthContribRegistry> {
    if let Some(r) = svcs.get::<HealthContribRegistry>() {
        return r;
    }
    let reg = Arc::new(HealthContribRegistry::default());
    svcs.register::<HealthContribRegistry>(reg.clone());
    reg
}

/// Helper to get or create the MetricsHookRegistry.
#[cfg(feature = "module")]
pub fn get_or_create_metrics_registry(
    svcs: &airframe_core::registry::ServiceRegistry,
) -> Arc<MetricsHookRegistry> {
    get_or_create_registry(svcs)
}

// --- Gateway-specific extension registries ---

/// Header policy hook for the gateway proxy. Allows modules to adjust headers on
/// outbound requests and inbound responses in order.
pub trait GatewayHeaderPolicy: Send + Sync {
    fn on_request(&self, _headers: &mut HeaderMap) {}
    fn on_response(&self, _headers: &mut HeaderMap) {}
}

/// Helper to get or create the GatewayHeaderPolicyRegistry.
#[cfg(feature = "module")]
pub fn get_or_create_gateway_header_policy_registry(
    svcs: &airframe_core::registry::ServiceRegistry,
) -> Arc<GatewayHeaderPolicyRegistry> {
    get_or_create_registry(svcs)
}

/// Path/URL rewrite hook for the gateway before dispatching upstream.
pub trait GatewayRewriter: Send + Sync {
    /// Given the configured upstream base, the matched tail, and the full incoming URI,
    /// return the full target URL to fetch.
    fn rewrite(&self, upstream_base: &str, tail: &str, uri: &http::Uri) -> String;
}

/// Helper to get or create the GatewayRewriterRegistry.
#[cfg(feature = "module")]
pub fn get_or_create_gateway_rewriter_registry(
    svcs: &airframe_core::registry::ServiceRegistry,
) -> Arc<GatewayRewriterRegistry> {
    get_or_create_registry(svcs)
}
