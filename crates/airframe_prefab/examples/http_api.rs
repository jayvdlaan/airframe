// Minimal HTTP API Server prefab example
// Requires the `http` feature:
//   cargo run -p airframe_prefab --features http --example http_api
// This mounts a simple /v1/ping route via the RouterContributor seam.

#![forbid(unsafe_code)]

#[cfg(feature = "http")]
use std::net::SocketAddr;
#[cfg(feature = "http")]
use std::sync::Arc;

#[cfg(feature = "http")]
use airframe_core::app::AppBuilder;
#[cfg(feature = "http")]
use airframe_core::module::{
    Module, ModuleContext, ModuleDescriptor, CAP_EXAMPLE_API, CAP_HTTP_SERVER,
};
#[cfg(feature = "http")]
use airframe_http::axum_server::{
    get_or_create_contrib_registry, OrderedRouterContributor, RouterContributor, RouterPhase,
};
#[cfg(feature = "http")]
use airframe_prefab::HttpApiServerPrefab;
#[cfg(feature = "http")]
use async_trait::async_trait;
#[cfg(feature = "http")]
use axum::routing::get;
#[cfg(feature = "http")]
use semver::Version;

// A tiny API module that contributes /v1/ping
#[cfg(feature = "http")]
pub struct ApiModule {
    desc: ModuleDescriptor,
}

#[cfg(feature = "http")]
impl ApiModule {
    pub fn new() -> Self {
        Self {
            desc: ModuleDescriptor {
                name: "example-api",
                version: Version::parse("0.1.0").unwrap(),
                provides: &[CAP_EXAMPLE_API.0],
                requires: &[],
                optional_requires: &[CAP_HTTP_SERVER.0],
                requires_with_versions: &[],
                optional_requires_with_versions: &[],
            },
        }
    }
}

#[cfg(feature = "http")]
struct ApiContributor;
#[cfg(feature = "http")]
impl RouterContributor for ApiContributor {
    fn mount(&self, router: axum::Router) -> axum::Router {
        use tower_http::cors::CorsLayer;
        // Demo: add permissive CORS for the example; real apps should configure properly.
        router
            .route("/v1/ping", get(|| async { "pong" }))
            .layer(CorsLayer::permissive())
    }
}
#[cfg(feature = "http")]
impl OrderedRouterContributor for ApiContributor {
    fn phase(&self) -> RouterPhase {
        RouterPhase::Routes
    }
    fn priority(&self) -> i32 {
        0
    }
}

#[cfg(feature = "http")]
#[async_trait]
impl Module for ApiModule {
    fn descriptor(&self) -> &ModuleDescriptor {
        &self.desc
    }
    async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> {
        // Only register contributor when HTTP server is present
        if ctx
            .services
            .get::<airframe_http::axum_server::BoundAddr>()
            .is_some()
        {
            let reg = get_or_create_contrib_registry(&ctx.services);
            reg.add(Arc::new(ApiContributor));
        }
        Ok(())
    }
}

#[cfg(feature = "http")]
#[tokio::main]
async fn main() {
    // Start from the HTTP API Server prefab
    let mut builder: AppBuilder = HttpApiServerPrefab::new();
    // Optionally override bind; prefab already binds to 127.0.0.1:8080 by default.
    let _bind: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    // Add our example API module
    builder = builder.with(ApiModule::new());
    // Start and run until shutdown
    match builder.start().await {
        Ok(app) => {
            let _ = app.run_until_cancelled().await;
        }
        Err(e) => eprintln!("HTTP API failed to start: {e}"),
    }
}

#[cfg(not(feature = "http"))]
fn main() {
    eprintln!("This example requires the 'http' feature. Run with: cargo run -p airframe_prefab --features http --example http_api");
}
