use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tower_http::services::ServeDir;

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_HTTP_SERVER};
use airframe_core::platform::PlatformSupport;
use airframe_http::axum_server::{
    get_or_create_contrib_registry, OrderedRouterContributor, RouterContributor, RouterPhase,
};
use airframe_macros::module_descriptor;

#[derive(Clone, Debug)]
pub struct StaticFilesConfig {
    /// Where to mount the static files in the router, e.g. "/" or "/assets".
    pub mount_path: String,
    /// Root directory on disk to serve.
    pub root_dir: PathBuf,
    /// If true, serve precompressed gzip files when available (".gz").
    pub precompress_gzip: bool,
    /// If true, serve precompressed brotli files when available (".br").
    pub precompress_br: bool,
    /// Reserved for future use: cache control behavior.
    pub cache_control: Option<String>,
}

impl StaticFilesConfig {
    pub fn new<P: AsRef<Path>>(mount_path: impl Into<String>, root_dir: P) -> Self {
        Self {
            mount_path: mount_path.into(),
            root_dir: root_dir.as_ref().to_path_buf(),
            precompress_gzip: true,
            precompress_br: true,
            cache_control: None,
        }
    }
}

pub struct StaticFilesModule {
    desc: ModuleDescriptor,
    cfg: StaticFilesConfig,
}

impl StaticFilesModule {
    pub fn new(cfg: StaticFilesConfig) -> Self {
        Self {
            desc: module_descriptor!(
                name: "http-static-files",
                version: "0.1.0",
                optional_requires: [CAP_HTTP_SERVER.0]
            ),
            cfg,
        }
    }
}

struct StaticFilesContributor(StaticFilesConfig);

impl RouterContributor for StaticFilesContributor {
    fn mount(&self, router: axum::Router) -> axum::Router {
        let mut sd = ServeDir::new(&self.0.root_dir);
        if self.0.precompress_gzip {
            sd = sd.precompressed_gzip();
        }
        if self.0.precompress_br {
            sd = sd.precompressed_br();
        }

        let svc = axum::routing::get_service(sd).handle_error(|err| async move {
            // Map io errors to 500
            use axum::http::StatusCode;
            tracing::warn!(error = %err, "serve_dir error");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error")
        });

        // Axum 0.8: nesting a service at "/" is no longer supported; use fallback_service instead.
        if self.0.mount_path == "/" {
            router.fallback_service(svc)
        } else {
            router.nest_service(&self.0.mount_path, svc)
        }
    }
}

impl OrderedRouterContributor for StaticFilesContributor {
    fn phase(&self) -> RouterPhase {
        RouterPhase::Routes
    }
    fn priority(&self) -> i32 {
        0
    }
}

#[async_trait]
impl Module for StaticFilesModule {
    airframe_macros::impl_descriptor!();

    fn platform_support(&self) -> PlatformSupport {
        PlatformSupport::desktop_only(
            "http prefab modules depend on an in-process HTTP server and are not supported on mobile targets",
        )
    }
    async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> {
        let reg = get_or_create_contrib_registry(&ctx.services);
        reg.add(Arc::new(StaticFilesContributor(self.cfg.clone())));
        Ok(())
    }
}
