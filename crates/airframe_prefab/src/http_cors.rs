//! HTTP CORS Module: contributes a global CORS layer and hot‑reloads on config changes.

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_HTTP_SERVER};
use airframe_core::platform::PlatformSupport;
use airframe_http::axum_server::get_or_create_layers_registry;
use airframe_macros::module_descriptor;
use async_trait::async_trait;
use axum::http::{HeaderValue, Method, StatusCode};
use axum::middleware::{from_fn, Next};
use axum::{extract::Request, response::Response};
use std::sync::Arc;

#[cfg(feature = "config")]
use airframe_config::{get_or_create_config_listener_registry, ConfigListener};

pub struct HttpCorsModule {
    desc: ModuleDescriptor,
}

impl Default for HttpCorsModule {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpCorsModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "http-cors",
                version: "0.1.0",
                optional_requires: [CAP_HTTP_SERVER.0]
            ),
        }
    }
}

async fn dynamic_cors_mw(
    req: Request,
    next: Next,
    rx: tokio::sync::watch::Receiver<bool>,
) -> Response {
    let enabled = *rx.borrow();
    let req_origin: Option<HeaderValue> = req
        .headers()
        .get("Origin")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| HeaderValue::from_str(s).ok());
    // Special handling of OPTIONS with minimal CORS when enabled
    if enabled && req.method() == Method::OPTIONS {
        let mut res = Response::builder().status(StatusCode::NO_CONTENT);
        let hv = req_origin
            .clone()
            .unwrap_or_else(|| HeaderValue::from_static("*"));
        res = res.header("access-control-allow-origin", hv);
        res = res
            .header(
                "access-control-allow-methods",
                HeaderValue::from_static("GET, POST, OPTIONS"),
            )
            .header(
                "access-control-allow-headers",
                HeaderValue::from_static("*"),
            )
            .header("vary", HeaderValue::from_static("Origin"));
        return res.body(axum::body::Body::empty()).unwrap();
    }
    let mut resp = next.run(req).await;
    if enabled {
        if let Some(origin) = resp
            .headers()
            .get("access-control-allow-origin")
            .cloned()
            .or_else(|| {
                req_origin
                    .clone()
                    .or_else(|| Some(HeaderValue::from_static("*")))
            })
        {
            let headers = resp.headers_mut();
            headers.insert("access-control-allow-origin", origin);
            headers.insert("vary", HeaderValue::from_static("Origin"));
        }
    }
    resp
}

#[async_trait]
impl Module for HttpCorsModule {
    airframe_macros::impl_descriptor!();

    fn platform_support(&self) -> PlatformSupport {
        PlatformSupport::desktop_only(
            "http prefab modules depend on an in-process HTTP server and are not supported on mobile targets",
        )
    }
    async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> {
        // Establish dynamic CORS state and global layer applier.
        let (tx, rx) = tokio::sync::watch::channel(false);
        // Seed from config if available
        #[cfg(feature = "config")]
        if let Some(cfg) = ctx
            .services
            .get::<airframe_config::api::types::BasicConfig>()
        {
            let cors_tbl = cfg.raw.get("cors");
            let enabled = cors_tbl
                .and_then(|t| t.get("enable"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let _ = tx.send_replace(enabled);
        }

        // Register global layer applier reading current flag
        let layers = get_or_create_layers_registry(&ctx.services);
        let rx_for_layer = rx.clone();
        layers.add(Arc::new(move |router: axum::Router| {
            let rx_clone = rx_for_layer.clone();
            router.layer(from_fn(move |req: Request, next: Next| {
                dynamic_cors_mw(req, next, rx_clone.clone())
            }))
        }));

        // Register as ConfigListener to update the flag on reloads
        #[cfg(feature = "config")]
        {
            struct CorsListener {
                tx: tokio::sync::watch::Sender<bool>,
            }
            impl ConfigListener for CorsListener {
                fn on_config_reload(&self, raw: &toml::Value) {
                    let enabled = raw
                        .get("cors")
                        .and_then(|t| t.get("enable"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let _ = self.tx.send(enabled);
                }
            }
            let reg = get_or_create_config_listener_registry(&ctx.services);
            reg.add(Arc::new(CorsListener { tx }));
        }
        #[cfg(not(feature = "config"))]
        {
            let _ = tx;
        }
        Ok(())
    }
}
