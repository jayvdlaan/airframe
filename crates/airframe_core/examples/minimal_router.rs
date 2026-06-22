use std::sync::Arc;

use airframe_core::app::AppBuilder;
use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_ROUTER};
use async_trait::async_trait;
use semver::Version;

// A minimal "Router" trait local to this example to simulate contributing a router.
trait Router: Send + Sync {
    fn route(&self, path: &str) -> String;
}

#[derive(Clone)]
struct SimpleRouter;
impl Router for SimpleRouter {
    fn route(&self, path: &str) -> String {
        format!("handled:{}", path)
    }
}

// Module that provides cap:router and registers a Router implementation in the ServiceRegistry.
struct RouterModule {
    desc: ModuleDescriptor,
}

#[async_trait]
impl Module for RouterModule {
    fn descriptor(&self) -> &ModuleDescriptor {
        &self.desc
    }
    async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> {
        // Register our simple router as a service so other modules (or main) can retrieve it.
        let svc: Arc<dyn Router> = Arc::new(SimpleRouter);
        ctx.services.register::<dyn Router>(svc);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Build an app with just our RouterModule.
    let router_mod = RouterModule {
        desc: ModuleDescriptor {
            name: "router",
            version: Version::parse("0.1.0")?,
            provides: &[CAP_ROUTER.0],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
    };

    let app = AppBuilder::new().with(router_mod).start().await?;

    // Fetch the router and use it.
    let router = app.services.get::<dyn Router>().expect("router service");
    println!("{}", router.route("/hello"));

    // Shut down immediately.
    app.cancel.cancel();
    Ok(())
}
