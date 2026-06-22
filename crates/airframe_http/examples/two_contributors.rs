// Feature-gated example: requires features "server" and "module"
#![cfg(all(feature = "server", feature = "module"))]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use airframe_core::app::AppBuilder;
use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_HTTP_SERVER};
use airframe_http::axum_server::{
    get_or_create_contrib_registry, AxumServerModule, BoundAddr, OrderedRouterContributor,
    RouterContributor, RouterPhase,
};
use anyhow::Result;
use async_trait::async_trait;
use axum::{routing::get, Router};
use semver::Version;

#[derive(Clone)]
struct StaticRoute(&'static str, &'static str);
impl RouterContributor for StaticRoute {
    fn mount(&self, router: Router) -> Router {
        let body = self.1;
        router.route(self.0, get(move || async move { body }))
    }
}
impl OrderedRouterContributor for StaticRoute {
    fn phase(&self) -> RouterPhase {
        RouterPhase::Routes
    }
    fn priority(&self) -> i32 {
        0
    }
}

struct ContribModule {
    desc: ModuleDescriptor,
    contribs: Vec<Arc<dyn OrderedRouterContributor>>,
}

#[async_trait]
impl Module for ContribModule {
    fn descriptor(&self) -> &ModuleDescriptor {
        &self.desc
    }
    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        let reg = get_or_create_contrib_registry(&ctx.services);
        for c in &self.contribs {
            reg.add(c.clone());
        }
        Ok(())
    }
}

fn module_desc(name: &'static str) -> ModuleDescriptor {
    ModuleDescriptor {
        name,
        version: Version::parse("0.1.0").unwrap(),
        provides: &[],
        requires: &[CAP_HTTP_SERVER.0],
        optional_requires: &[],
        requires_with_versions: &[],
        optional_requires_with_versions: &[],
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Start server on an ephemeral port
    let server = AxumServerModule::new(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0));

    let a = Arc::new(StaticRoute("/a", "A")) as Arc<dyn OrderedRouterContributor>;
    let b = Arc::new(StaticRoute("/b", "B")) as Arc<dyn OrderedRouterContributor>;

    let contribs = ContribModule {
        desc: module_desc("contrib"),
        contribs: vec![a, b],
    };

    let app = AppBuilder::new()
        .with(contribs)
        .with(server)
        .start()
        .await?;

    let addr = app.services.get::<BoundAddr>().expect("BoundAddr").0;
    eprintln!(
        "Serving on http://{}:{} (GET /a -> A, /b -> B). Ctrl+C to stop.",
        addr.ip(),
        addr.port()
    );

    app.run_until_cancelled().await?;
    Ok(())
}
