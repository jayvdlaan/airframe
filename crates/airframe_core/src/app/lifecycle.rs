//! Application lifecycle: bootstrap, the [`AppBuilder`] assembly entry point,
//! and the running [`AppHandle`] (start/shutdown and capability resolution).
//!
//! The dependency graph and resolver that `start()` relies on live in the
//! sibling [`super::graph`] module.

use std::sync::Arc;

use anyhow::{bail, Result};
use tokio_util::sync::CancellationToken;
use tracing::{info, Span};

use crate::bus::inmem::{InMemoryCommandBus, InMemoryEventBus, InMemoryQueryBus};
use crate::module::{Module, ModuleContext};
use crate::platform::current_platform;
use crate::registry::ServiceRegistry;

/// Minimal bootstrap options executed before modules initialize.
#[derive(Debug, Clone, Default)]
pub struct Bootstrap {
    /// If true, install a minimal stderr logger early using tracing-subscriber.
    pub minimal_logger: bool,
}

impl Bootstrap {
    /// Run the bootstrap steps and, if installing a minimal logger, return a guard that keeps
    /// a thread-local default subscriber active until dropped. Best-effort; never panics.
    pub fn install(&self) -> Option<tracing::dispatcher::DefaultGuard> {
        if self.minimal_logger {
            #[allow(unused_imports)]
            use tracing_subscriber::{fmt, prelude::*, registry::Registry, EnvFilter};
            let fmt_layer = fmt::layer()
                .with_target(false)
                .with_level(true)
                .with_writer(std::io::stderr);
            let filter = EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("warn,airframe=info"));
            let subscriber = Registry::default().with(filter).with(fmt_layer);
            // Install as a thread-local default so the real logging module can later install a
            // global subscriber without conflict. Keep the guard alive until we're ready to drop.
            let guard = tracing::dispatcher::set_default(&tracing::Dispatch::new(subscriber));
            Some(guard)
        } else {
            None
        }
    }
}

pub struct AppBuilder {
    pub(super) modules: Vec<Box<dyn Module>>, // simple linear order for now
    pub(super) bootstrap: Option<Bootstrap>,
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AppBuilder {
    pub fn new() -> Self {
        Self {
            modules: vec![],
            bootstrap: None,
        }
    }

    pub fn with<M: Module + 'static>(mut self, m: M) -> Self {
        self.modules.push(Box::new(m));
        self
    }

    /// Opt-in bootstrap configuration that runs at the very start of `start()`.
    pub fn with_bootstrap(mut self, bootstrap: Bootstrap) -> Self {
        self.bootstrap = Some(bootstrap);
        self
    }

    pub async fn start(mut self) -> Result<AppHandle> {
        // Early bootstrap: keep guard alive for the duration of start() so very-early logs are handled
        let _bootstrap_guard = if let Some(b) = &self.bootstrap {
            b.install()
        } else {
            None
        };
        info!(target = "airframe_core", "app starting");

        // Fail fast if any selected module is not supported on the current platform.
        let platform = current_platform();
        for m in &self.modules {
            let support = m.platform_support();
            if !support.allows(platform) {
                let name = m.descriptor().name;
                if let Some(reason) = support.reason() {
                    bail!("module \"{name}\" is not supported on {platform}: {reason}");
                }
                bail!("module \"{name}\" is not supported on {platform}");
            }
        }

        // Construct shared services and buses
        let services = ServiceRegistry::default();
        let events: Arc<InMemoryEventBus> = Arc::new(InMemoryEventBus::new());
        let commands: Arc<InMemoryCommandBus> = Arc::new(InMemoryCommandBus::default());
        let queries: Arc<InMemoryQueryBus> = Arc::new(InMemoryQueryBus::default());
        let cancel = CancellationToken::new();

        // Register buses into the service registry for module discovery
        services.register::<InMemoryEventBus>(events.clone());
        services.register::<InMemoryCommandBus>(commands.clone());
        services.register::<InMemoryQueryBus>(queries.clone());

        let typed_bus: Arc<crate::bus::typed::TypedEventBus> =
            Arc::new(crate::bus::typed::TypedEventBus::new());
        services.register::<crate::bus::typed::TypedEventBus>(typed_bus);

        let base_ctx = ModuleContext {
            services: services.clone(),
            cancel: cancel.clone(),
            span: Span::current(),
        };

        // Optional: validate layering before resolving dependencies
        #[cfg(feature = "layer-check")]
        {
            self.validate_layers()?;
        }
        // Resolve dependency order
        let order = Self::resolve_dependencies(&self.modules)?;

        // init and start in dependency order
        for &idx in &order {
            self.modules[idx].init(base_ctx.clone()).await?;
        }
        for &idx in &order {
            self.modules[idx].start().await?;
        }

        // Reorder internal modules vector to dependency order for correct shutdown sequence
        let mut slots: Vec<Option<Box<dyn Module>>> = self.modules.into_iter().map(Some).collect();
        let mut ordered_modules: Vec<Box<dyn Module>> = Vec::with_capacity(order.len());
        for idx in order {
            ordered_modules.push(slots[idx].take().expect("module present"));
        }

        Ok(AppHandle {
            modules: ordered_modules,
            services,
            events,
            commands,
            queries,
            cancel,
        })
    }
}

pub struct AppHandle {
    modules: Vec<Box<dyn Module>>,
    pub services: ServiceRegistry,
    pub events: Arc<InMemoryEventBus>,
    pub commands: Arc<InMemoryCommandBus>,
    pub queries: Arc<InMemoryQueryBus>,
    pub cancel: CancellationToken,
}

impl AppHandle {
    pub async fn shutdown(&mut self) -> Result<()> {
        info!(target = "airframe_core", "app shutdown requested");
        // stop in reverse dependency order: current vec order matches start order
        for m in self.modules.iter_mut().rev() {
            m.stop().await?;
        }
        Ok(())
    }

    pub async fn run_until_cancelled(mut self) -> Result<()> {
        tokio::select! {
            _ = self.cancel.cancelled() => {},
            _ = tokio::signal::ctrl_c() => {},
        }
        // Ensure all tasks observing the cancellation token are notified before shutdown
        self.cancel.cancel();
        self.shutdown().await
    }

    /// Request cancellation after the given duration. Useful for graceful shutdown deadlines.
    pub fn cancel_after(&self, dur: std::time::Duration) {
        let token = self.cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(dur).await;
            token.cancel();
        });
    }

    /// Resolve a typed capability handle from the service registry.
    ///
    /// This is a convenience wrapper around `services.resolve::<T>()`.
    /// The returned `CapabilityHandle<T>` dereferences directly to `T`
    /// for zero-cost access after the initial resolve.
    pub fn resolve<T: Send + Sync + 'static>(
        &self,
    ) -> Result<crate::registry::CapabilityHandle<T>> {
        self.services.resolve::<T>()
    }

    /// Resolve the `TypedEventBus` from the service registry.
    pub fn typed_event_bus(
        &self,
    ) -> Result<crate::registry::CapabilityHandle<crate::bus::typed::TypedEventBus>> {
        self.services.resolve::<crate::bus::typed::TypedEventBus>()
    }
}
