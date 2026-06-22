// Requires features server+module to run
#![cfg(all(feature = "server", feature = "module"))]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use airframe_core::app::AppBuilder;
use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_HTTP_SERVER};
use airframe_http::axum_server::{
    get_or_create_contrib_registry, get_or_create_error_mapper_registry,
    get_or_create_layers_registry, AxumServerModule, BoundAddr, ErrorMapperRegistry,
    GlobalLayerRegistry, OrderedRouterContributor, RouterContributor, RouterPhase,
};
use anyhow::Result;
use async_trait::async_trait;
use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use axum::{
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
    Router,
};
use semver::Version;
use tower::Layer as _; // ensure layer() method is available

// Generic layer contributor appending a name to x-order
#[derive(Clone)]
struct LayerContrib {
    name: &'static str,
    phase: RouterPhase,
    prio: i32,
}
impl RouterContributor for LayerContrib {
    fn mount(&self, router: Router) -> Router {
        let name = self.name;
        router.layer(axum::middleware::from_fn(
            move |mut req: Request<Body>, next: Next| {
                let name = name;
                async move {
                    let cur = req
                        .headers()
                        .get("x-order")
                        .and_then(|h| h.to_str().ok())
                        .unwrap_or("");
                    let newv = if cur.is_empty() {
                        name.to_string()
                    } else {
                        format!("{cur}|{name}")
                    };
                    req.headers_mut().insert(
                        "x-order",
                        HeaderValue::from_str(&newv).unwrap_or(HeaderValue::from_static(name)),
                    );
                    next.run(req).await
                }
            },
        ))
    }
}
impl OrderedRouterContributor for LayerContrib {
    fn phase(&self) -> RouterPhase {
        self.phase
    }
    fn priority(&self) -> i32 {
        self.prio
    }
}

// Route contributor providing /ping and /err endpoints
#[derive(Clone)]
struct PingRoutes;
impl RouterContributor for PingRoutes {
    fn mount(&self, router: Router) -> Router {
        router
            .route(
                "/ping",
                get(|req: Request<Body>| async move {
                    let cur = req
                        .headers()
                        .get("x-order")
                        .and_then(|h| h.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    Response::new(Body::from(cur))
                }),
            )
            .route(
                "/err",
                get(|| async move { (StatusCode::INTERNAL_SERVER_ERROR, "boom") }),
            )
    }
}
impl OrderedRouterContributor for PingRoutes {
    fn phase(&self) -> RouterPhase {
        RouterPhase::Routes
    }
    fn priority(&self) -> i32 {
        0
    }
}

struct ContribsModule {
    desc: ModuleDescriptor,
    list: Vec<Arc<dyn OrderedRouterContributor>>,
    register_global: bool,
    register_errmap: bool,
}

#[async_trait]
impl Module for ContribsModule {
    fn descriptor(&self) -> &ModuleDescriptor {
        &self.desc
    }
    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        let reg = get_or_create_contrib_registry(&ctx.services);
        for c in &self.list {
            reg.add(c.clone());
        }
        if self.register_global {
            let layers: Arc<GlobalLayerRegistry> = get_or_create_layers_registry(&ctx.services);
            layers.add(Arc::new(|r: Router| {
                r.layer(axum::middleware::from_fn(
                    |req: Request<Body>, next: Next| async move {
                        let mut resp = next.run(req).await;
                        resp.headers_mut()
                            .insert("x-global", HeaderValue::from_static("1"));
                        resp
                    },
                ))
            }));
        }
        if self.register_errmap {
            let errs: Arc<ErrorMapperRegistry> = get_or_create_error_mapper_registry(&ctx.services);
            errs.add(Arc::new(|mut resp: Response| {
                // Transform a 500 into 200 and mark header
                if resp.status() == StatusCode::INTERNAL_SERVER_ERROR {
                    *resp.status_mut() = StatusCode::OK;
                    resp.headers_mut()
                        .insert("x-mapped", HeaderValue::from_static("1"));
                }
                resp
            }));
        }
        Ok(())
    }
}

fn desc(name: &'static str) -> ModuleDescriptor {
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

#[tokio::test(flavor = "multi_thread")]
async fn contributors_mount_in_phase_order_and_layers_wrap_with_priorities() {
    let server = AxumServerModule::new(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0));
    // Build contributors across all phases with priorities
    let list: Vec<Arc<dyn OrderedRouterContributor>> = vec![
        Arc::new(LayerContrib {
            name: "pre-10",
            phase: RouterPhase::PreLayers,
            prio: -10,
        }),
        Arc::new(LayerContrib {
            name: "pre0",
            phase: RouterPhase::PreLayers,
            prio: 0,
        }),
        Arc::new(LayerContrib {
            name: "pre10",
            phase: RouterPhase::PreLayers,
            prio: 10,
        }),
        Arc::new(LayerContrib {
            name: "route0",
            phase: RouterPhase::Routes,
            prio: 0,
        }),
        Arc::new(LayerContrib {
            name: "route5",
            phase: RouterPhase::Routes,
            prio: 5,
        }),
        Arc::new(LayerContrib {
            name: "post0",
            phase: RouterPhase::PostLayers,
            prio: 0,
        }),
        Arc::new(LayerContrib {
            name: "fb0",
            phase: RouterPhase::Fallback,
            prio: 0,
        }),
        Arc::new(PingRoutes),
    ];
    let contribs = ContribsModule {
        desc: desc("contribs"),
        list,
        register_global: false,
        register_errmap: false,
    };

    let app = AppBuilder::new()
        .with(contribs)
        .with(server)
        .start()
        .await
        .expect("start");
    let addr = app.services.get::<BoundAddr>().expect("addr").0;

    let client = reqwest::Client::builder().no_proxy().build().unwrap();

    // Call /ping and read body which reflects X-Order header mutations.
    // Sorted mount: Pre(-10,0,10), Routes(0,5, handler), Post(0), Fallback(0).
    // Axum applies a layer only to routes present at the time of layer mounting. Since handler is mounted
    // in Routes phase with priority 0, only layers mounted AFTER it will affect the request.
    // That yields: Fallback(0) -> Post(0) -> Routes(5). Within-phase priority order respected (5 after 0).
    let url = format!("http://{}/ping", addr);
    let resp = client.get(&url).send().await.expect("resp");
    assert!(resp.status().is_success());
    let body = resp.text().await.unwrap_or_default();
    assert_eq!(body, "fb0|post0|route5");

    // Shutdown
    let mut app = app;
    app.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn global_layer_wraps_and_error_mapper_maps() {
    let server = AxumServerModule::new(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0));
    let list: Vec<Arc<dyn OrderedRouterContributor>> = vec![Arc::new(PingRoutes)];
    let contribs = ContribsModule {
        desc: desc("with_globals"),
        list,
        register_global: true,
        register_errmap: true,
    };

    let app = AppBuilder::new()
        .with(contribs)
        .with(server)
        .start()
        .await
        .expect("start");
    let addr = app.services.get::<BoundAddr>().expect("addr").0;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();

    // Global layer should apply to all routes
    let url = format!("http://{}/ping", addr);
    let resp = client.get(&url).send().await.expect("resp");
    assert!(resp.status().is_success());
    assert_eq!(
        resp.headers().get("x-global").and_then(|h| h.to_str().ok()),
        Some("1")
    );

    // Error mapper should transform a 500 into 200 with header marker
    let url = format!("http://{}/err", addr);
    let resp = client.get(&url).send().await.expect("resp");
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("x-mapped").and_then(|h| h.to_str().ok()),
        Some("1")
    );

    let mut app = app;
    app.shutdown().await.unwrap();
}
