//! HTTP SPA Module: serves a directory with SPA-style fallback to index.html.

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_HTTP_SERVER};
use airframe_core::platform::PlatformSupport;
use airframe_http::axum_server::{
    get_or_create_contrib_registry, OrderedRouterContributor, RouterContributor, RouterPhase,
};
use airframe_macros::module_descriptor;
use async_trait::async_trait;
use axum::http::{
    header::{CACHE_CONTROL, CONTENT_TYPE},
    HeaderValue,
};
use axum::middleware::{from_fn, Next};
use axum::{extract::Request, response::Html, response::Response, routing::get};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};

/// Cache policy for SPA responses. The asset file names are fixed (not
/// content-hashed), so without explicit headers a browser/WebView serves a stale
/// cached bundle indefinitely. Two tiers:
///
/// * The **HTML shell** (`index.html`, served directly and as the SPA navigation
///   fallback) is `no-store` — never cached. It carries the references to the
///   fixed-name JS/wasm/CSS, so a redeploy must be reflected immediately;
///   otherwise a stale cached shell masks the new bundle (and inline scripts)
///   even on a reload. The shell is tiny, so never caching it is cheap.
/// * **Everything else** (wasm/JS/CSS/fonts, and any API JSON passing through)
///   is `no-cache` — cacheable but always revalidated, so `ServeDir` answers
///   conditional requests with `304 Not Modified` and the large wasm bundle is
///   only re-downloaded when it actually changes.
///
/// Only sets the header when a handler hasn't already set its own.
async fn revalidate_assets(req: Request, next: Next) -> Response {
    let mut resp = next.run(req).await;
    let is_html = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/html"));
    let policy = if is_html { "no-store" } else { "no-cache" };
    resp.headers_mut()
        .entry(CACHE_CONTROL)
        .or_insert(HeaderValue::from_static(policy));
    resp
}

#[derive(Clone, Debug)]
pub struct SpaConfig {
    /// Root directory on disk to serve (typically your build/dist folder)
    pub root_dir: PathBuf,
    /// Mount path for the SPA, usually "/".
    pub mount_path: String,
    /// Index file name used for fallback, default "index.html".
    pub index_file: String,
    /// Prefer precompressed gzip assets when available.
    pub precompress_gzip: bool,
    /// Prefer precompressed brotli assets when available.
    pub precompress_br: bool,
}

impl SpaConfig {
    pub fn new<P: AsRef<Path>>(root_dir: P) -> Self {
        Self {
            root_dir: root_dir.as_ref().to_path_buf(),
            mount_path: "/".to_string(),
            index_file: "index.html".to_string(),
            precompress_gzip: true,
            precompress_br: true,
        }
    }
}

pub struct SpaModule {
    desc: ModuleDescriptor,
    cfg: SpaConfig,
}

impl SpaModule {
    pub fn new(cfg: SpaConfig) -> Self {
        Self {
            desc: module_descriptor!(
                name: "http-spa",
                version: "0.1.0",
                optional_requires: [CAP_HTTP_SERVER.0]
            ),
            cfg,
        }
    }
}

struct SpaContributor(SpaConfig);

impl RouterContributor for SpaContributor {
    fn mount(&self, router: axum::Router) -> axum::Router {
        // Diagnostics: log resolved root/index so users can quickly spot path issues
        let root = self.0.root_dir.clone();
        let index_path = root.join(&self.0.index_file);
        let root_exists = root.exists();
        let index_exists = index_path.exists();
        tracing::info!(
            target = "airframe_prefab::http_spa",
            mount = %self.0.mount_path,
            root = %root.display(),
            index = %index_path.display(),
            root_exists = root_exists,
            index_exists = index_exists,
            "mounting SPA"
        );

        // If the target directory (or index) does not exist, provide a helpful in-memory placeholder
        // so the application still responds on "/" with guidance.
        if !root_exists || !index_exists {
            tracing::warn!(
                target = "airframe_prefab::http_spa",
                mount = %self.0.mount_path,
                root = %root.display(),
                index = %index_path.display(),
                "SPA root or index not found; serving placeholder page. You can set NANOPASS_WEB_DIST or adjust SpaConfig::new(...) to point at your built assets."
            );

            let handler = get(move || async move {
                let html = format!(
                    "{}{}{}",
                    PLACEHOLDER_PREFIX,
                    escape_html(&root.display().to_string()),
                    PLACEHOLDER_SUFFIX,
                );
                Html(html)
            });

            if self.0.mount_path == "/" {
                return router.fallback(handler);
            } else {
                return router.route(&self.0.mount_path, handler);
            }
        }

        let mut dir = ServeDir::new(&root);
        if self.0.precompress_gzip {
            dir = dir.precompressed_gzip();
        }
        if self.0.precompress_br {
            dir = dir.precompressed_br();
        }
        let index = ServeFile::new(index_path);
        let svc = dir.fallback(index);
        // Axum 0.8: nesting a service at "/" is no longer supported, use fallback_service instead.
        let router = if self.0.mount_path == "/" {
            router.fallback_service(svc)
        } else {
            router.nest_service(&self.0.mount_path, svc)
        };
        // Make clients revalidate assets so updated bundles aren't masked by a
        // stale cache (fixed asset names + heuristic caching = stuck builds).
        router.layer(from_fn(revalidate_assets))
    }
}

impl OrderedRouterContributor for SpaContributor {
    fn phase(&self) -> RouterPhase {
        RouterPhase::Routes
    }
    fn priority(&self) -> i32 {
        0
    }
}

#[async_trait]
impl Module for SpaModule {
    airframe_macros::impl_descriptor!();

    fn platform_support(&self) -> PlatformSupport {
        PlatformSupport::desktop_only(
            "http prefab modules depend on an in-process HTTP server and are not supported on mobile targets",
        )
    }
    async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> {
        let reg = get_or_create_contrib_registry(&ctx.services);
        reg.add(Arc::new(SpaContributor(self.cfg.clone())));
        Ok(())
    }
}

// Small, self-contained placeholder HTML shown when the target SPA directory is missing at runtime.
// Includes guidance for configuring the correct path.
const PLACEHOLDER_PREFIX: &str = r#"<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>SPA not found</title><style>html,body{height:100%;margin:0;font-family:system-ui,-apple-system,Segoe UI,Roboto,Ubuntu,Cantarell,Noto Sans,sans-serif;} .wrap{min-height:100%;display:grid;place-items:center;background:#0b1020;color:#e6e8ee;} .card{padding:2rem 2.5rem;border-radius:12px;background:#141a33;box-shadow:0 10px 30px rgba(0,0,0,.25);max-width:800px} code{background:#0e1330;padding:.2rem .4rem;border-radius:6px} a{color:#7db4ff} .muted{color:#aab0c0}</style><div class=\"wrap\"><div class=\"card\"><h1>Static assets not found</h1><p class=\"muted\">Airframe's SPA module could not find your built assets directory or index.html.</p><p>Expected at: <code>"#;
const PLACEHOLDER_SUFFIX: &str = r#"</code></p><p>How to fix:</p><ul><li>Ensure your frontend is built and available inside the runtime image/container.</li><li>Or set the <code>NANOPASS_WEB_DIST</code> environment variable to the absolute path of your build directory.</li><li>Or update <code>SpaConfig::new(...)</code> in your app to point at the correct folder.</li></ul><p class=\"muted\">This is a developer-friendly placeholder. Once the build directory exists, the SPA will be served here.</p></div></div></html>"#;

#[inline]
fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}
