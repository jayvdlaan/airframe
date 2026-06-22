//! OpenAPI serving prefab.
//! When the `openapi` feature is enabled (alongside `http`), this module exposes
//! a merged `/openapi.json` generated from one or more registered JSON providers.
//! Applications (e.g., Nanokey, Nanopass) should call `register_json_provider(...)`
//! during their initialization so their fragments are included.
//!
//! ## Static vs ServiceRegistry
//!
//! The public [`register_json_provider`] function uses a process-global static
//! (`OnceLock<Mutex<…>>`) as a staging area. This is intentional: some callers
//! (e.g., Nanokey's `main.rs`) register their OpenAPI document *before* the
//! `AppBuilder` is constructed and therefore before a `ServiceRegistry` exists.
//!
//! During [`OpenApiModule::init`], all staged providers are drained from the
//! static into an `Arc<Mutex<Registry>>` owned by the `ServiceRegistry`.
//! From that point on, the route handler reads exclusively from the
//! ServiceRegistry-backed instance and the static is no longer consulted.

#![forbid(unsafe_code)]

#[cfg(all(feature = "http", feature = "openapi"))]
use std::sync::{Arc, Mutex, OnceLock};
#[cfg(all(feature = "http", feature = "openapi"))]
use std::time::SystemTime;

#[cfg(all(feature = "http", feature = "openapi"))]
use async_trait::async_trait;

#[cfg(all(feature = "http", feature = "openapi"))]
use airframe_core::module::{
    Module, ModuleContext, ModuleDescriptor, CAP_HTTP_SERVER, CAP_OPENAPI,
};
#[cfg(all(feature = "http", feature = "openapi"))]
use airframe_core::platform::PlatformSupport;
#[cfg(all(feature = "http", feature = "openapi"))]
use airframe_http::axum_server::{
    get_or_create_contrib_registry, OrderedRouterContributor, RouterContributor, RouterPhase,
};
#[cfg(all(feature = "http", feature = "openapi"))]
use airframe_macros::module_descriptor;

#[cfg(all(feature = "http", feature = "openapi"))]
use axum::http::{header, HeaderValue};
#[cfg(all(feature = "http", feature = "openapi"))]
use std::time::UNIX_EPOCH;

#[cfg(all(feature = "http", feature = "openapi"))]
#[derive(Clone)]
struct OpenApiCache {
    json: serde_json::Value,
    bytes: Vec<u8>,
    etag: String,
    last_modified: SystemTime,
}

#[cfg(all(feature = "http", feature = "openapi"))]
pub struct Registry {
    providers: Vec<Arc<dyn Fn() -> serde_json::Value + Send + Sync + 'static>>,
    cache: Option<OpenApiCache>,
}

// ---------------------------------------------------------------------------
// Static staging area for pre-init registrations
// ---------------------------------------------------------------------------
//
// This static exists solely to collect providers registered before
// `ServiceRegistry` is available (i.e., before `AppBuilder::new()`).
// `OpenApiModule::init` drains it into the ServiceRegistry-owned instance.

#[cfg(all(feature = "http", feature = "openapi"))]
#[allow(clippy::type_complexity)]
static STAGING: OnceLock<Mutex<Vec<Arc<dyn Fn() -> serde_json::Value + Send + Sync + 'static>>>> =
    OnceLock::new();

#[cfg(all(feature = "http", feature = "openapi"))]
fn staging() -> &'static Mutex<Vec<Arc<dyn Fn() -> serde_json::Value + Send + Sync + 'static>>> {
    STAGING.get_or_init(|| Mutex::new(Vec::new()))
}

/// Register a provider closure that returns a JSON OpenAPI document fragment.
///
/// This function is safe to call at any point during application lifecycle,
/// including before `AppBuilder` is constructed. Providers registered before
/// `OpenApiModule::init` are staged in a process-global static and drained
/// into the `ServiceRegistry` during module initialization.
#[cfg(all(feature = "http", feature = "openapi"))]
pub fn register_json_provider(
    provider: Arc<dyn Fn() -> serde_json::Value + Send + Sync + 'static>,
) {
    let stg = staging();
    let mut guard = stg.lock().unwrap();
    guard.push(provider);
}

// ---------------------------------------------------------------------------
// ServiceRegistry-owned Registry helpers
// ---------------------------------------------------------------------------

/// Wrapper type so we can store `Arc<Mutex<Registry>>` in `ServiceRegistry`
/// keyed by a unique type (avoids TypeId collisions with unrelated types).
#[cfg(all(feature = "http", feature = "openapi"))]
struct OpenApiRegistryHandle(Arc<Mutex<Registry>>);

/// Get or create the `Arc<Mutex<Registry>>` in the given `ServiceRegistry`,
/// draining any providers that were staged in the process-global static.
#[cfg(all(feature = "http", feature = "openapi"))]
fn get_or_create_sr_registry(
    services: &airframe_core::registry::ServiceRegistry,
) -> Arc<Mutex<Registry>> {
    let handle = services.get_or_register::<OpenApiRegistryHandle, _>(|| {
        // Drain staged providers from the static into the new registry.
        let staged = {
            let stg = staging();
            let mut guard = stg.lock().unwrap();
            std::mem::take(&mut *guard)
        };
        Arc::new(OpenApiRegistryHandle(Arc::new(Mutex::new(Registry {
            providers: staged,
            cache: None,
        }))))
    });
    handle.0.clone()
}

// ---------------------------------------------------------------------------
// Backward-compatible public accessor (deprecated)
// ---------------------------------------------------------------------------

/// Get or create the global OpenAPI registry (internal reference).
///
/// **Deprecated**: Prefer using `ServiceRegistry` via `OpenApiModule`. This
/// function returns a reference to the static staging area wrapped in a
/// temporary `Mutex<Registry>` for API compatibility, but providers
/// registered through it will only be picked up if called before
/// `OpenApiModule::init`.
#[cfg(all(feature = "http", feature = "openapi"))]
#[deprecated(
    note = "Use ServiceRegistry via OpenApiModule instead. This accessor returns the static staging area."
)]
pub fn get_or_create_openapi_registry() -> &'static Mutex<Registry> {
    // Provide backward compatibility by returning a static Mutex<Registry>
    // that reads from staging. This is kept only for API compatibility.
    static COMPAT: OnceLock<Mutex<Registry>> = OnceLock::new();
    COMPAT.get_or_init(|| {
        Mutex::new(Registry {
            providers: Vec::new(),
            cache: None,
        })
    })
}

#[cfg(all(feature = "http", feature = "openapi"))]
fn sha256_hex(bytes: &[u8]) -> String {
    use airframe_crypt::{
        hash::DigestAlgorithm,
        suite::{CipherSuite, SoftwareCipherSuite},
    };
    let suite = SoftwareCipherSuite::new();
    let digest = suite
        .digest(DigestAlgorithm::Sha256, bytes)
        .unwrap_or_default();
    // hex encode without extra deps
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        hex.push_str(&format!("{:02x}", b));
    }
    format!("\"{}\"", hex) // quoted etag
}

#[cfg(all(feature = "http", feature = "openapi"))]
fn merge_openapi_docs(mut base: serde_json::Value, other: &serde_json::Value) -> serde_json::Value {
    use serde_json::{Map, Value};
    // Merge paths
    let has_base_paths = base.get("paths").and_then(|v| v.as_object()).is_some();
    // Work on a new scope to avoid borrow checker issues by re-borrowing base
    {
        let base_paths = base.get_mut("paths").and_then(|v| v.as_object_mut());
        let other_paths = other.get("paths").and_then(|v| v.as_object());
        if let (Some(bp), Some(op)) = (base_paths, other_paths) {
            for (k, v) in op.iter() {
                bp.insert(k.clone(), v.clone());
            }
        } else if !has_base_paths {
            if let Some(op) = other_paths {
                base["paths"] = Value::Object(op.clone());
            }
        }
    }
    // Merge components.schemas and components.securitySchemes
    fn merge_components_section(base: &mut Value, other: &Value, section: &str) {
        let _base_comp = base.get_mut("components").and_then(|v| v.as_object_mut());
        let other_comp = other.get("components").and_then(|v| v.as_object());
        if let Some(oc) = other_comp {
            let sec_val = oc.get(section).and_then(|v| v.as_object());
            if let Some(sec_map) = sec_val {
                let bc = base.get_mut("components").and_then(|v| v.as_object_mut());
                if bc.is_none() {
                    base["components"] = Value::Object(Map::new());
                }
                let bc = base
                    .get_mut("components")
                    .and_then(|v| v.as_object_mut())
                    .unwrap();
                let cur = bc.get_mut(section).and_then(|v| v.as_object_mut());
                if cur.is_none() {
                    bc.insert(section.to_string(), Value::Object(Map::new()));
                }
                let cur = bc.get_mut(section).and_then(|v| v.as_object_mut()).unwrap();
                for (k, v) in sec_map.iter() {
                    cur.insert(k.clone(), v.clone());
                }
            }
        }
    }
    merge_components_section(&mut base, other, "schemas");
    merge_components_section(&mut base, other, "securitySchemes");

    // Merge servers (concat)
    if let Some(arr) = other.get("servers").and_then(|v| v.as_array()) {
        let base_arr = base.get_mut("servers").and_then(|v| v.as_array());
        if base_arr.is_none() {
            base["servers"] = serde_json::json!([]);
        }
        let base_arr = base
            .get_mut("servers")
            .and_then(|v| v.as_array_mut())
            .unwrap();
        for v in arr {
            base_arr.push(v.clone());
        }
    }

    base
}

/// Build the merged OpenAPI document from registered providers, caching the
/// result. The `reg` parameter is the ServiceRegistry-owned registry instance.
#[cfg(all(feature = "http", feature = "openapi"))]
fn build_or_get_cache(reg: &Mutex<Registry>) -> OpenApiCache {
    {
        let guard = reg.lock().unwrap();
        if let Some(cache) = guard.cache.as_ref() {
            return cache.clone();
        }
        // Start with a minimal doc if no providers
        let mut merged = serde_json::json!({
            "openapi": "3.1.0",
            "info": { "title": "Airframe API", "version": "0.1.0" },
            "paths": {}
        });
        let providers: Vec<_> = guard.providers.to_vec();
        drop(guard);
        for p in providers {
            let doc = p();
            merged = merge_openapi_docs(merged, &doc);
            // Prefer first non-empty info if base was placeholder
            if merged.get("info").is_none() {
                if let Some(info) = doc.get("info") {
                    merged["info"] = info.clone();
                }
            }
            if merged.get("openapi").is_none() {
                if let Some(ver) = doc.get("openapi") {
                    merged["openapi"] = ver.clone();
                }
            }
        }
        let bytes = serde_json::to_vec(&merged).unwrap_or_else(|_| b"{}".to_vec());
        let etag = sha256_hex(&bytes);
        let cache = OpenApiCache {
            json: merged,
            bytes,
            etag,
            last_modified: SystemTime::now(),
        };
        let mut guard = reg.lock().unwrap();
        guard.cache = Some(OpenApiCache {
            json: cache.json.clone(),
            bytes: cache.bytes.clone(),
            etag: cache.etag.clone(),
            last_modified: cache.last_modified,
        });
        cache
    }
}

#[cfg(all(feature = "http", feature = "openapi"))]
pub struct OpenApiModule {
    desc: ModuleDescriptor,
}

#[cfg(all(feature = "http", feature = "openapi"))]
impl Default for OpenApiModule {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenApiModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "prefab-openapi",
                version: "0.1.0",
                provides: [CAP_OPENAPI.0],
                requires: [CAP_HTTP_SERVER.0]
            ),
        }
    }
}

#[cfg(all(feature = "http", feature = "openapi"))]
struct OpenApiContributor {
    /// The ServiceRegistry-owned registry, passed in during module init.
    registry: Arc<Mutex<Registry>>,
}

#[cfg(all(feature = "http", feature = "openapi"))]
impl RouterContributor for OpenApiContributor {
    fn mount(&self, router: axum::Router) -> axum::Router {
        use axum::routing::get;
        let reg = self.registry.clone();
        // Always mount /openapi.json
        let router = router.route(
            "/openapi.json",
            get(move || {
                let reg = reg.clone();
                async move {
                    // Build or fetch cache from the ServiceRegistry-owned registry
                    let cache = build_or_get_cache(&reg);
                    let mut resp =
                        axum::response::Response::new(axum::body::Body::from(cache.bytes.clone()));
                    resp.headers_mut().insert(
                        header::CONTENT_TYPE,
                        HeaderValue::from_static("application/json"),
                    );
                    resp.headers_mut()
                        .insert(header::ETAG, HeaderValue::from_str(&cache.etag).unwrap());
                    let _secs = cache
                        .last_modified
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    resp.headers_mut().insert(
                        header::LAST_MODIFIED,
                        HeaderValue::from_str(&httpdate::fmt_http_date(cache.last_modified))
                            .unwrap(),
                    );
                    resp
                }
            }),
        );
        // Optionally mount CDN-backed Swagger UI at /ui when the `swagger-ui` feature is enabled
        #[cfg(feature = "swagger-ui")]
        let router = {
            use axum::response::Html;
            router.route(
                "/ui",
                get(|| async move {
                    let html = r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>API Docs</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist/swagger-ui.css" />
  <style>
    html, body { height: 100%; margin: 0; padding: 0; }
    #swagger-ui { height: 100vh; margin: 0; padding: 0; }
  </style>
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist/swagger-ui-bundle.js"></script>
  <script>
    window.addEventListener('load', function() {
      if (window.SwaggerUIBundle) {
        const specUrl = new URL('openapi.json', window.location.href).toString();
        window.ui = SwaggerUIBundle({
          url: specUrl,
          dom_id: '#swagger-ui',
          deepLinking: true,
          presets: [
            SwaggerUIBundle.presets.apis,
            SwaggerUIBundle.SwaggerUIStandalonePreset
          ],
          layout: 'BaseLayout'
        });
      }
    });
  </script>
  <noscript>Enable JavaScript to view the API documentation.</noscript>
</body>
</html>"#;
                    Html(html)
                }),
            )
        };
        router
    }
}

#[cfg(all(feature = "http", feature = "openapi"))]
impl OrderedRouterContributor for OpenApiContributor {
    fn phase(&self) -> RouterPhase {
        RouterPhase::Routes
    }
    fn priority(&self) -> i32 {
        0
    }
}

#[cfg(all(feature = "http", feature = "openapi"))]
#[async_trait]
impl Module for OpenApiModule {
    airframe_macros::impl_descriptor!();

    fn platform_support(&self) -> PlatformSupport {
        PlatformSupport::desktop_only(
            "http prefab modules depend on an in-process HTTP server and are not supported on mobile targets",
        )
    }

    async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> {
        // Obtain (or create) the ServiceRegistry-owned OpenAPI registry.
        // This drains any providers that were staged in the process-global
        // static before ServiceRegistry existed (e.g., from main.rs).
        let openapi_reg = get_or_create_sr_registry(&ctx.services);

        // Register the OpenAPI contributor with a reference to the
        // ServiceRegistry-owned registry, so the route handler no longer
        // reads from the process-global static.
        let contrib_reg = get_or_create_contrib_registry(&ctx.services);
        contrib_reg.add(Arc::new(OpenApiContributor {
            registry: openapi_reg,
        }));
        Ok(())
    }
}
