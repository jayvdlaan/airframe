#![forbid(unsafe_code)]

// Integration test for HTTP API Server prefab route composition.
// Requires `--features http` when running tests for this crate:
//   cargo test -p airframe_prefab --features http -- --nocapture

#[cfg(feature = "http")]
mod http_routes {
    use std::sync::Arc;

    use airframe_core::app::AppBuilder;
    use airframe_core::module::{
        Module, ModuleContext, ModuleDescriptor, CAP_HTTP_SERVER, CAP_TEST_API,
    };
    use airframe_http::axum_server::{
        get_or_create_contrib_registry, BoundAddr, OrderedRouterContributor, RouterContributor,
        RouterPhase,
    };
    use airframe_prefab::HttpApiServerPrefab;
    use async_trait::async_trait;
    use axum::routing::get;
    use semver::Version;

    // Tiny API module that contributes /v1/ping
    struct ApiModule {
        desc: ModuleDescriptor,
    }
    impl ApiModule {
        fn new() -> Self {
            Self {
                desc: ModuleDescriptor {
                    name: "test-api",
                    version: Version::parse("0.1.0").unwrap(),
                    provides: &[CAP_TEST_API.0],
                    requires: &[],
                    optional_requires: &[CAP_HTTP_SERVER.0],
                    requires_with_versions: &[],
                    optional_requires_with_versions: &[],
                },
            }
        }
    }

    struct ApiContributor;
    impl RouterContributor for ApiContributor {
        fn mount(&self, router: axum::Router) -> axum::Router {
            router.route("/v1/ping", get(|| async { "pong" }))
        }
    }
    impl OrderedRouterContributor for ApiContributor {
        fn phase(&self) -> RouterPhase {
            RouterPhase::Routes
        }
        fn priority(&self) -> i32 {
            0
        }
    }

    #[async_trait]
    impl Module for ApiModule {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> {
            if ctx.services.get::<BoundAddr>().is_some() {
                let reg = get_or_create_contrib_registry(&ctx.services);
                reg.add(Arc::new(ApiContributor));
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn http_prefab_mounts_contributor_routes() {
        // Build from the HTTP API Server prefab
        let builder: AppBuilder = HttpApiServerPrefab::new().with(ApiModule::new());
        let app = builder.start().await.expect("app should start");

        // Discover the bound address
        let addr = app
            .services
            .get::<BoundAddr>()
            .expect("BoundAddr registered")
            .0;
        let url = format!("http://{}/v1/ping", addr);

        // HTTP client without proxy and small timeout
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("client build");

        // Readiness loop (handle port-0 races)
        let mut ok = false;
        let mut last_err: Option<anyhow::Error> = None;
        for _ in 0..50 {
            // ~5s
            match client.get(&url).send().await {
                Ok(resp) => {
                    assert!(
                        resp.status().is_success(),
                        "expected 2xx, got {:?}",
                        resp.status()
                    );
                    let body = resp.text().await.unwrap_or_default();
                    assert!(body.contains("pong"));
                    ok = true;
                    break;
                }
                Err(e) => {
                    last_err = Some(anyhow::anyhow!(e));
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
        if !ok {
            panic!("failed to GET /v1/ping: {:?}", last_err);
        }

        // Shutdown
        let mut app = app;
        app.shutdown().await.expect("shutdown ok");
    }
}

#[cfg(not(feature = "http"))]
#[test]
fn http_feature_required() {
    // No-op: required features are not enabled
}
