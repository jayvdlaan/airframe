// Module integration for airframe_config: ConfigModule implementation lives here.
// This module is compiled only when feature = "module" is enabled.

#![cfg(feature = "module")]

use std::path::PathBuf;
use std::sync::Arc;

use airframe_core::bus::EventBus;
#[cfg(feature = "args")]
use airframe_core::module::CAP_ARGS;
use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_CONFIG};
use anyhow::Result;
use async_trait::async_trait;
use semver::Version;
use tracing::{debug, info, instrument};

use crate::api::types::{BasicConfig, ConfigReloaded};
#[cfg(feature = "module")]
use crate::config_listener::ConfigListenerRegistry;
#[cfg(feature = "module")]
use crate::defaults_registry::ConfigDefaultsRegistry;
use crate::io::cli::config_paths_from_args;
use crate::io::cli::merge_from_cli;
use crate::io::env::merge_from_env_with_prefixes;
use crate::io::files::{merge_toml, read_files};

/// Configuration module wiring for Airframe runtime.
pub struct ConfigModule {
    desc: ModuleDescriptor,
    pub default_path: Option<PathBuf>,
    defaults: Option<toml::Value>,
    #[allow(clippy::type_complexity)]
    validator: Option<Arc<dyn Fn(&toml::Value) -> Result<()> + Send + Sync>>,
    cli_overrides: Option<Vec<String>>, // for tests and embedding
    hot_reload: bool,
    strict_file_selection: bool,
    env_prefixes: Vec<String>,
}

impl ConfigModule {
    pub fn new(default_path: Option<PathBuf>) -> Self {
        // Feature-gated hard requirement on cap:args to guarantee CLI availability before config builds
        // Do not hard-require cap:args; consume it opportunistically when present.
        // This keeps ConfigModule usable in contexts without CLI (tests, embedded apps).
        #[cfg(feature = "args")]
        const OPTIONAL: &[&str] = &[CAP_ARGS.0];
        #[cfg(not(feature = "args"))]
        const OPTIONAL: &[&str] = &[];
        Self {
            desc: ModuleDescriptor {
                name: "config",
                version: Version::parse("0.1.0").unwrap(),
                provides: &[CAP_CONFIG.0],
                requires: &[],
                optional_requires: OPTIONAL,
                requires_with_versions: &[],
                optional_requires_with_versions: &[],
            },
            default_path,
            defaults: None,
            validator: None,
            cli_overrides: None,
            // File watching and polling loops are generally not appropriate on mobile targets.
            // Keep the module usable (static config load), but disable hot reload by default.
            hot_reload: !cfg!(any(target_os = "android", target_os = "ios")),
            strict_file_selection: false,
            env_prefixes: vec![],
        }
    }

    pub fn with_defaults(mut self, defaults: toml::Value) -> Self {
        self.defaults = Some(defaults);
        self
    }
    pub fn with_validator<F>(mut self, f: F) -> Self
    where
        F: Fn(&toml::Value) -> Result<()> + Send + Sync + 'static,
    {
        self.validator = Some(Arc::new(f));
        self
    }
    pub fn with_cli_overrides(mut self, overrides: Vec<String>) -> Self {
        self.cli_overrides = Some(overrides);
        self
    }
    pub fn with_hot_reload(mut self, enabled: bool) -> Self {
        self.hot_reload = enabled;
        self
    }
    pub fn with_strict_file_selection(mut self, enabled: bool) -> Self {
        self.strict_file_selection = enabled;
        self
    }
    /// Configure which env prefixes are recognized during merge, e.g., ["NANOKEY__"].
    pub fn with_env_prefixes<S: Into<String>>(mut self, prefixes: Vec<S>) -> Self {
        self.env_prefixes = prefixes.into_iter().map(|s| s.into()).collect();
        self
    }
}

#[async_trait]
impl Module for ConfigModule {
    airframe_macros::impl_descriptor!();
    #[instrument(level = "info", skip(self, ctx))]
    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        // Guard: even if explicitly enabled, hot reload should not run on mobile targets.
        // The file-watcher loop is long-lived and can be battery-hostile.
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            if self.hot_reload {
                anyhow::bail!(
                    "config hot-reload is not supported on mobile targets; disable it with ConfigModule::with_hot_reload(false)"
                );
            }
        }

        // was: info!(target = "airframe_config", "initializing config module");
        // Layering: (registry defaults ->) defaults -> file(s) -> env -> CLI
        let mut raw = self
            .defaults
            .clone()
            .unwrap_or_else(|| toml::Value::Table(Default::default()));

        // Merge in any registered defaults contributors first (lowest precedence, left-to-right)
        if let Some(reg) = ctx.services.get::<ConfigDefaultsRegistry>() {
            for c in reg.all() {
                let v = c.defaults();
                merge_toml(&mut raw, v);
            }
        }

        // Resolve file paths from CLI (if available), then env, then default
        // Prefer explicit test/embedding overrides if provided; otherwise, fall back to ArgsModule when present.
        let cli_paths_from_overrides: Option<Vec<PathBuf>> = self
            .cli_overrides
            .as_ref()
            .and_then(|ov| config_paths_from_args(ov));
        #[cfg(feature = "args")]
        let cli_paths_from_service: Option<Vec<PathBuf>> = ctx
            .services
            .get::<airframe_args::CliArgs>()
            .and_then(|args| config_paths_from_args(&args.argv));
        #[cfg(not(feature = "args"))]
        let cli_paths_from_service: Option<Vec<PathBuf>> = None;
        let cli_paths: Option<Vec<PathBuf>> = cli_paths_from_overrides.or(cli_paths_from_service);

        let env_path = std::env::var("AIRFRAME_CONFIG_PATH").ok();
        let file_paths: Vec<PathBuf> =
            crate::resolve::resolve_paths(cli_paths.clone(), env_path, self.default_path.clone());
        if !file_paths.is_empty() {
            let list: Vec<String> = file_paths.iter().map(|p| p.display().to_string()).collect();
            debug!(target = "airframe_config", sources = %list.join(","), "config merged");
        }

        // If strict mode and CLI explicitly provided paths, validate existence
        if self.strict_file_selection && cli_paths.is_some() {
            for p in &file_paths {
                if !p.exists() {
                    anyhow::bail!(
                        "config file specified on CLI does not exist: {}",
                        p.display()
                    );
                }
            }
        }

        if !file_paths.is_empty() {
            let files_val = read_files(&file_paths);
            merge_toml(&mut raw, files_val);
        }

        // Env overrides
        merge_from_env_with_prefixes(&mut raw, &self.env_prefixes);

        // CLI overrides: from provided overrides or from CliArgs service if present
        if let Some(overrides) = &self.cli_overrides {
            merge_from_cli(&mut raw, overrides);
        }
        #[cfg(feature = "args")]
        {
            if self.cli_overrides.is_none() {
                if let Some(args) = ctx.services.get::<airframe_args::CliArgs>() {
                    merge_from_cli(&mut raw, &args.argv);
                }
            }
        }

        // Validate if validator provided
        if let Some(v) = &self.validator {
            v(&raw)?;
        }

        // Source: if exactly one file path, record it; else None
        let source = if file_paths.len() == 1 {
            Some(file_paths[0].clone())
        } else {
            None
        };
        let cfg = BasicConfig { raw, source };
        ctx.services.register::<BasicConfig>(Arc::new(cfg));
        if let Some(path) = ctx
            .services
            .get::<BasicConfig>()
            .and_then(|bc| bc.source.clone())
        {
            info!(target = "airframe_config", path = %path.display(), "config loaded");
        }

        // Publish ConfigReloaded after a short delay to let subscribers subscribe during their init phase.
        // A too‑short delay (e.g., 1ms) has shown to be flaky on CI due to scheduler variance.
        // Use a more conservative delay to improve determinism for tests and startup listeners.
        if let Some(bus) = ctx
            .services
            .get::<airframe_core::bus::inmem::InMemoryEventBus>()
        {
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                let _ = bus.publish(ConfigReloaded, None).await;
            });
        }

        // Also broadcast to ConfigListenerRegistry shortly after initial load
        if let Some(reg) = ctx.services.get::<ConfigListenerRegistry>() {
            if let Some(cfg) = ctx.services.get::<BasicConfig>() {
                let listeners = reg.all();
                let raw = cfg.raw.clone();
                tokio::spawn(async move {
                    for l in listeners.into_iter() {
                        l.on_config_reload(&raw);
                    }
                });
            }
        }

        // Subscribe to ConfigReloaded bus and broadcast to listeners on every event.
        if let Some(bus) = ctx
            .services
            .get::<airframe_core::bus::inmem::InMemoryEventBus>()
        {
            let services = ctx.services.clone();
            airframe_core::bus::spawn_event_watcher::<_, ConfigReloaded, _, _>(
                bus.as_ref(),
                ctx.cancel.clone(),
                move |_evt| {
                    let services = services.clone();
                    async move {
                        if let (Some(reg), Some(cfg)) = (
                            services.get::<ConfigListenerRegistry>(),
                            services.get::<BasicConfig>(),
                        ) {
                            let listeners = reg.all();
                            let raw = cfg.raw.clone();
                            tokio::spawn(async move {
                                for l in listeners.into_iter() {
                                    l.on_config_reload(&raw);
                                }
                            });
                        }
                    }
                },
            )?;
        }

        // Hot-reload: If a single source file was used, watch it for changes and reload with debounce.
        if self.hot_reload {
            if let Some(src_path) = ctx
                .services
                .get::<BasicConfig>()
                .and_then(|bc| bc.source.clone())
            {
                let services = ctx.services.clone();
                let cancel = ctx.cancel.clone();
                let defaults_for_reload = self.defaults.clone();
                let cli_overrides_for_reload = self.cli_overrides.clone();

                // was: info!(target = "airframe_config", path = %src_path.display(), "watching config for reload");
                crate::reload::spawn_watcher(
                    src_path,
                    services,
                    cancel,
                    defaults_for_reload,
                    cli_overrides_for_reload,
                    self.env_prefixes.clone(),
                );
            }
        }
        Ok(())
    }
}
