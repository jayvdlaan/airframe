#![forbid(unsafe_code)]

// Basic error mapping test: verifies a contributed route can return a 400 JSON error and is surfaced by the HTTP API Server prefab.
// Run with:
//   cargo test -p airframe_prefab --features http --test http_error -- --nocapture

#[cfg(feature = "http")]
mod http_error {
    use std::sync::Arc;

    use airframe_core::app::AppBuilder;
    use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_HTTP_SERVER};
    use airframe_http::axum_server::{
        get_or_create_contrib_registry, BoundAddr, OrderedRouterContributor, RouterContributor,
        RouterPhase,
    };
    use airframe_prefab::HttpApiServerPrefab;
    use async_trait::async_trait;
    use axum::{http::StatusCode, response::IntoResponse, Json};
    use semver::Version;
    use serde_json::json;

    struct ErrorApiModule {
        desc: ModuleDescriptor,
    }
    impl ErrorApiModule {
        fn new() -> Self {
            Self {
                desc: ModuleDescriptor {
                    name: "error-api",
                    version: Version::parse("0.1.0").unwrap(),
                    provides: &[],
                    requires: &[],
                    optional_requires: &[CAP_HTTP_SERVER.0],
                    requires_with_versions: &[],
                    optional_requires_with_versions: &[],
                },
            }
        }
    }

    struct ErrorContrib;
    impl RouterContributor for ErrorContrib {
        fn mount(&self, router: axum::Router) -> axum::Router {
            router.route(
                "/v1/fail",
                axum::routing::get(|| async move {
                    let body = json!({"error":"bad_request","message":"invalid input"});
                    (StatusCode::BAD_REQUEST, Json(body)).into_response()
                }),
            )
        }
    }
    impl OrderedRouterContributor for ErrorContrib {
        fn phase(&self) -> RouterPhase {
            RouterPhase::Routes
        }
        fn priority(&self) -> i32 {
            0
        }
    }

    #[async_trait]
    impl Module for ErrorApiModule {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> {
            if ctx.services.get::<BoundAddr>().is_some() {
                let reg = get_or_create_contrib_registry(&ctx.services);
                reg.add(Arc::new(ErrorContrib));
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn contributed_route_returns_json_error() {
        // Build from the HTTP API Server prefab and add our error route
        let builder: AppBuilder = HttpApiServerPrefab::new().with(ErrorApiModule::new());
        let app = builder.start().await.expect("app starts");

        // Discover bound address
        let addr = app
            .services
            .get::<BoundAddr>()
            .expect("BoundAddr present")
            .0;

        // Client
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("client");

        // Readiness loop
        let url = format!("http://{}/v1/fail", addr);
        let mut resp = None;
        for _ in 0..50 {
            match client.get(&url).send().await {
                Ok(r) => {
                    resp = Some(r);
                    break;
                }
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
            }
        }
        let resp = resp.expect("got response");
        assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
        let json: serde_json::Value = resp.json().await.expect("json body");
        assert_eq!(json["error"], "bad_request");
        assert_eq!(json["message"], "invalid input");

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
