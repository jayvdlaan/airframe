//! Gateway module: minimal reverse proxy with prefix routing (HTTP feature).

mod headers;
mod proxy;

use std::sync::Arc;

use airframe_core::bus::EventBus;
use airframe_core::module::{
    Module, ModuleContext, ModuleDescriptor, CAP_CONFIG, CAP_GATEWAY, CAP_HTTP_SERVER,
};
use airframe_core::platform::PlatformSupport;
use airframe_http::axum_server::{
    get_or_create_contrib_registry, get_or_create_gateway_rewriter_registry,
    OrderedRouterContributor, RouterContributor, RouterPhase,
};
use anyhow::Result;
use async_trait::async_trait;
use axum::{routing::any, Router};
use futures_util::StreamExt as _;
use semver::Version;

use proxy::{proxy_handler, DefaultRewriter};

#[derive(Clone, Debug)]
struct Route {
    prefix: String,
    upstream: String,
}

#[derive(Clone, Default)]
struct RouteTable {
    routes: Arc<Vec<Route>>,
}

impl RouteTable {
    fn match_upstream(&self, path: &str) -> Option<(String, String)> {
        // Assumes routes are sorted by descending prefix length; first match wins
        for r in self.routes.iter() {
            let p = &r.prefix;
            if path == p
                || (path.starts_with(p) && (p == "/" || path.chars().nth(p.len()) == Some('/')))
            {
                let tail = &path[p.len()..];
                let mut upstream = r.upstream.clone();
                if upstream.ends_with('/') && tail.starts_with('/') {
                    upstream.pop();
                }
                return Some((upstream, tail.to_string()));
            }
        }
        None
    }
}

struct GatewayContributor {
    services: airframe_core::registry::ServiceRegistry,
    client: reqwest::Client,
    #[allow(dead_code)]
    hclient: Option<
        hyper_util::client::legacy::Client<
            hyper_util::client::legacy::connect::HttpConnector,
            http_body_util::Empty<axum::body::Bytes>,
        >,
    >, // HTTP-only zero-copy for GET/HEAD
    routes_rx: tokio::sync::watch::Receiver<RouteTable>,
    max_body_bytes: usize,
    streaming: bool,
    zero_copy: bool,
}

impl RouterContributor for GatewayContributor {
    fn mount(&self, router: Router) -> Router {
        // Mount a catch-all under / to proxy. Keep health route from server as-is.
        let services = self.services.clone();
        let client = self.client.clone();
        let hclient = self.hclient.clone();
        let routes_rx = self.routes_rx.clone();
        let max_body_bytes = self.max_body_bytes;
        let streaming = self.streaming;
        let zero_copy = self.zero_copy;
        router.route(
            "/{*path}",
            any(move |req| {
                proxy_handler(
                    req,
                    services.clone(),
                    routes_rx.clone(),
                    client.clone(),
                    hclient.clone(),
                    max_body_bytes,
                    streaming,
                    zero_copy,
                )
            }),
        )
    }
}

impl OrderedRouterContributor for GatewayContributor {
    fn phase(&self) -> RouterPhase {
        RouterPhase::Fallback
    }
    fn priority(&self) -> i32 {
        0
    }
}

/// GatewayModule reads config and registers a RouterContributor that proxies
/// according to a simple prefix routing table.
pub struct GatewayModule {
    desc: ModuleDescriptor,
}

impl Default for GatewayModule {
    fn default() -> Self {
        Self::new()
    }
}

impl GatewayModule {
    pub fn new() -> Self {
        Self {
            desc: ModuleDescriptor {
                name: "gateway",
                version: Version::parse("0.1.0").unwrap(),
                provides: &[CAP_GATEWAY.0],
                // Hard require HTTP server capability (any version) for simplicity; keep versioned tuple in a static below if needed.
                requires: &[CAP_HTTP_SERVER.0],
                optional_requires: &[CAP_CONFIG.0],
                requires_with_versions: GW_HTTP_REQS,
                optional_requires_with_versions: &[],
            },
        }
    }
}

// Versioned HTTP requirement (optional enforcement by the app builder). Using a 'static to avoid temporary lifetime issues.
const GW_HTTP_REQS: &[(&str, &str)] = &[(CAP_HTTP_SERVER.0, ">=0.1.0")];

#[async_trait]
impl Module for GatewayModule {
    airframe_macros::impl_descriptor!();

    fn platform_support(&self) -> PlatformSupport {
        PlatformSupport::desktop_only(
            "gateway module depends on an in-process HTTP server and is not supported on mobile targets",
        )
    }

    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        // Read config for timeouts and limits; also streaming and zero-copy toggle
        #[allow(unused_variables)]
        let (connect_timeout_ms, request_timeout_ms, max_body_bytes, streaming, zero_copy) = {
            #[cfg(feature = "config")]
            {
                let cfg = ctx
                    .services
                    .get::<airframe_config::api::types::BasicConfig>();
                if let Some(cfg) = cfg {
                    let g = cfg.raw.get("gateway");
                    let ct = g
                        .and_then(|t| t.get("connect_timeout_ms"))
                        .and_then(|v| v.as_integer())
                        .unwrap_or(1000) as u64;
                    let rt = g
                        .and_then(|t| t.get("request_timeout_ms"))
                        .and_then(|v| v.as_integer())
                        .unwrap_or(5000) as u64;
                    let mb = g
                        .and_then(|t| t.get("max_body_bytes"))
                        .and_then(|v| v.as_integer())
                        .unwrap_or(2 * 1024 * 1024) as usize;
                    let st = g
                        .and_then(|t| t.get("streaming"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let zc = g
                        .and_then(|t| t.get("zero_copy_http"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    (ct, rt, mb, st, zc)
                } else {
                    (1000, 5000, 2 * 1024 * 1024, false, false)
                }
            }
            #[cfg(not(feature = "config"))]
            {
                (1000, 5000, 2 * 1024 * 1024, false, false)
            }
        };

        // Build HTTP client
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_millis(connect_timeout_ms))
            .timeout(std::time::Duration::from_millis(request_timeout_ms))
            .no_proxy()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        // Optional hyper client for zero-copy HTTP (no TLS)
        let hclient = {
            if zero_copy {
                let mut http = hyper_util::client::legacy::connect::HttpConnector::new();
                http.enforce_http(false);
                Some(
                    hyper_util::client::legacy::Client::builder(
                        hyper_util::rt::TokioExecutor::new(),
                    )
                    .build(http),
                )
            } else {
                None
            }
        };

        // Build initial route table and hot-reload on config changes
        let (_routes_tx, routes_rx) = {
            #[cfg(feature = "config")]
            {
                let initial = if let Some(cfg) = ctx
                    .services
                    .get::<airframe_config::api::types::BasicConfig>()
                {
                    build_table_from_config(&cfg.raw)
                } else {
                    RouteTable::default()
                };
                let (tx, rx) = tokio::sync::watch::channel(initial);
                // Subscribe to ConfigReloaded and rebuild
                if let Some(bus) = ctx
                    .services
                    .get::<airframe_core::bus::inmem::InMemoryEventBus>()
                {
                    let services = ctx.services.clone();
                    let mut sub = bus
                        .subscribe::<airframe_config::api::types::ConfigReloaded>()
                        .expect("subscribe");
                    let tx_clone = tx.clone();
                    tokio::spawn(async move {
                        while sub.next().await.is_some() {
                            if let Some(cfg) =
                                services.get::<airframe_config::api::types::BasicConfig>()
                            {
                                let table = build_table_from_config(&cfg.raw);
                                let _ = tx_clone.send(table);
                            }
                        }
                    });
                }
                (tx, rx)
            }
            #[cfg(not(feature = "config"))]
            {
                let (tx, rx) = tokio::sync::watch::channel(RouteTable::default());
                (tx, rx)
            }
        };

        // Register default rewriter; header policy has a built-in default in handler when none registered
        {
            let rw = get_or_create_gateway_rewriter_registry(&ctx.services);
            rw.add(Arc::new(DefaultRewriter));
        }

        let reg = get_or_create_contrib_registry(&ctx.services);
        reg.add(Arc::new(GatewayContributor {
            services: ctx.services.clone(),
            client,
            hclient,
            routes_rx,
            max_body_bytes,
            streaming,
            zero_copy,
        }));
        Ok(())
    }
}

#[allow(dead_code)]
fn build_table_from_config(raw: &toml::Value) -> RouteTable {
    let mut routes: Vec<Route> = Vec::new();
    if let Some(tbl) = raw.get("gateway") {
        if let Some(arr) = tbl.get("routes").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(prefix) = item.get("path_prefix").and_then(|v| v.as_str()) {
                    if let Some(up) = item.get("upstream").and_then(|v| v.as_str()) {
                        routes.push(Route {
                            prefix: normalize_prefix(prefix),
                            upstream: up.to_string(),
                        });
                    }
                }
            }
        }
    }
    // Sort by descending prefix length for faster first-match
    routes.sort_by(|a, b| b.prefix.len().cmp(&a.prefix.len()));
    RouteTable {
        routes: Arc::new(routes),
    }
}

// --- Helpers: path normalization ---

fn normalize_path(p: &str) -> String {
    // ensure leading slash, collapse repeats, keep trailing as-is
    let mut out = String::with_capacity(p.len());
    let bytes = p.as_bytes();
    if bytes.is_empty() || bytes[0] != b'/' {
        out.push('/');
    }
    let mut last_was_slash = false;
    for ch in p.chars() {
        if ch == '/' {
            if !last_was_slash {
                out.push('/');
                last_was_slash = true;
            }
        } else {
            out.push(ch);
            last_was_slash = false;
        }
    }
    out
}

/// Normalize configured route prefixes for matching:
/// - ensure leading slash
/// - collapse duplicate slashes
/// - drop trailing slash for non-root so both "/api" and "/api/" match
#[allow(dead_code)]
fn normalize_prefix(p: &str) -> String {
    let mut s = normalize_path(p);
    if s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    s
}

// Intentionally no default GatewayHeaderPolicy registration; see apply_default_forward_headers in proxy module.
