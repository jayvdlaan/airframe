use std::sync::Arc;

use axum::{routing::get, Json, Router};
use serde_json::json;

use airframe_core::module::{
    Module, ModuleContext, ModuleDescriptor, CAP_HTTP_ROUTER_ADMIN, CAP_HTTP_SERVER,
};
use airframe_core::platform::PlatformSupport;
use airframe_macros::module_descriptor;
use async_trait::async_trait;
use tracing::debug;

use crate::axum_server::{
    get_or_create_contrib_registry, OrderedRouterContributor, RouterContributor, RouterPhase,
};

/// Contributes standard Admin routes under /admin
#[derive(Clone)]
pub struct AdminContributor {
    pub name: &'static str,
    pub version: &'static str,
}

impl RouterContributor for AdminContributor {
    fn mount(&self, router: Router) -> Router {
        let name = self.name;
        let version = self.version;

        let health = || async { Json(json!({ "status": "ok" })) };

        let version_handler = move || {
            let name = name;
            let version = version;
            async move { Json(json!({ "name": name, "version": version })) }
        };

        let openapi_json = move || {
            let version = version;
            async move {
                let doc = json!({
                    "openapi": "3.0.0",
                    "info": { "title": "Admin API", "version": version },
                    "paths": {
                        "/admin/health": { "get": { "operationId": "health", "responses": { "200": { "description": "OK" } } } },
                        "/admin/version": { "get": { "operationId": "version", "responses": { "200": { "description": "Version" } } } },
                        "/admin/openapi.json": { "get": { "operationId": "openapi", "responses": { "200": { "description": "Spec" } } } }
                    }
                });
                Json(doc)
            }
        };

        router
            .route("/admin/health", get(health))
            .route("/admin/version", get(version_handler))
            .route("/admin/openapi.json", get(openapi_json))
    }
}

impl OrderedRouterContributor for AdminContributor {
    fn phase(&self) -> RouterPhase {
        RouterPhase::Routes
    }
    fn priority(&self) -> i32 {
        0
    }
}

/// Admin module that registers the AdminContributor into the Axum RouterContribRegistry.
pub struct AdminModule {
    desc: ModuleDescriptor,
    contrib: Arc<AdminContributor>,
}

impl AdminModule {
    pub fn new(name: &'static str, version: &'static str) -> Self {
        Self {
            desc: module_descriptor!(
                name: "admin",
                version: "0.1.0",
                provides: [CAP_HTTP_ROUTER_ADMIN.0],
                requires: [CAP_HTTP_SERVER.0]
            ),
            contrib: Arc::new(AdminContributor { name, version }),
        }
    }

    /// Convenience to build a CodeSpec describing the admin endpoints.
    pub fn codespec(base: airframe_api::http::Uri) -> airframe_api::CodeSpec {
        use airframe_api::http::Method;
        airframe_api::CodeSpec::new(base)
            .route("health", Method::GET, "/admin/health")
            .route("version", Method::GET, "/admin/version")
            .route("openapi", Method::GET, "/admin/openapi.json")
    }
}

#[async_trait]
impl Module for AdminModule {
    airframe_macros::impl_descriptor!();

    fn platform_support(&self) -> PlatformSupport {
        PlatformSupport::desktop_only(
            "admin module depends on an in-process HTTP server and is not supported on mobile targets",
        )
    }

    async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> {
        let reg = get_or_create_contrib_registry(&ctx.services);
        debug!(
            target = "airframe_http",
            name = self.contrib.name,
            version = self.contrib.version,
            "registering AdminContributor"
        );
        reg.add(self.contrib.clone());
        Ok(())
    }
}
