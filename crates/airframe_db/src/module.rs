// Airframe runtime integration for airframe_db.
// Compiled only with feature = "module" (gated at the `mod module;` site in lib.rs).

use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::FutureExt;
use tracing::{debug, info, instrument};

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_DB, CAP_HEALTH};
use airframe_core::registry::ServiceRegistry;
use airframe_macros::module_descriptor;

use crate::connection::{DbConnection, DbPool, Migrator};

// --- Config surface ---
#[derive(Debug, Clone, Default)]
pub struct DbPoolConfig {
    pub max_size: Option<u32>,
    pub connect_timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum DbDriverId {
    #[default]
    Sqlite,
    Mysql,
}
impl DbDriverId {
    pub fn as_str(self) -> &'static str {
        match self {
            DbDriverId::Sqlite => "sqlite",
            DbDriverId::Mysql => "mysql",
        }
    }
    // Returns Option (not Result), so deliberately not std::str::FromStr.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "sqlite" => Some(DbDriverId::Sqlite),
            "mysql" => Some(DbDriverId::Mysql),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DbConfig {
    pub driver: DbDriverId,
    pub url: Option<String>,
    pub pool: DbPoolConfig,
    pub migrations_path: Option<String>,
    pub migrations_on_start: MigrationsMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum MigrationsMode {
    Run,
    #[default]
    Skip,
}
impl MigrationsMode {
    pub fn as_str(self) -> &'static str {
        match self {
            MigrationsMode::Run => "run",
            MigrationsMode::Skip => "skip",
        }
    }
    // Returns Option (not Result), so deliberately not std::str::FromStr.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "run" => Some(MigrationsMode::Run),
            "skip" => Some(MigrationsMode::Skip),
            _ => None,
        }
    }
}

impl DbConfig {
    fn from_registry(services: &ServiceRegistry) -> Self {
        // Prefer reading from airframe_config::BasicConfig if available; else defaults
        #[allow(unused_mut)]
        let mut cfg = DbConfig::default();
        if let Some(bc) = services.get::<airframe_config::api::types::BasicConfig>() {
            let raw = &bc.raw;
            // strings like db.driver, db.url
            if let Some(tbl) = raw.get("db").and_then(|v| v.as_table()) {
                if let Some(drv) = tbl.get("driver").and_then(|v| v.as_str()) {
                    cfg.driver = DbDriverId::from_str(drv).unwrap_or_default();
                }
                if let Some(url) = tbl.get("url").and_then(|v| v.as_str()) {
                    cfg.url = Some(url.to_string());
                }
                if let Some(pool) = tbl.get("pool").and_then(|v| v.as_table()) {
                    if let Some(ms) = pool.get("max_size").and_then(|v| v.as_integer()) {
                        cfg.pool.max_size = Some(ms as u32);
                    }
                    if let Some(to) = pool.get("connect_timeout_ms").and_then(|v| v.as_integer()) {
                        cfg.pool.connect_timeout_ms = Some(to as u64);
                    }
                }
                if let Some(mig) = tbl.get("migrations").and_then(|v| v.as_table()) {
                    if let Some(path) = mig.get("path").and_then(|v| v.as_str()) {
                        cfg.migrations_path = Some(path.to_string());
                    }
                    if let Some(on_start) = mig.get("on_start").and_then(|v| v.as_str()) {
                        cfg.migrations_on_start =
                            MigrationsMode::from_str(on_start).unwrap_or_default();
                    }
                }
            }
        }
        cfg
    }
}

// --- Registered handle ---
/// Wrapper type to register a database pool in the ServiceRegistry.
pub struct DbHandle<P: DbPool>(pub P);

// --- Module ---
pub struct DbModule {
    desc: ModuleDescriptor,
}

impl Default for DbModule {
    fn default() -> Self {
        Self::new()
    }
}

impl DbModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "db",
                version: "0.1.0",
                provides: [CAP_DB.0],
                optional_requires: [CAP_HEALTH.0]
            ),
        }
    }
}

#[async_trait]
impl Module for DbModule {
    airframe_macros::impl_descriptor!();

    #[instrument(level = "info", skip(self, ctx), target = "airframe_db")]
    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        let cfg = DbConfig::from_registry(&ctx.services);
        let redacted_url = cfg
            .url
            .as_deref()
            .map(crate::config::redact_url)
            .unwrap_or_else(|| "(default)".to_string());
        debug!(target = "airframe_db", driver = %cfg.driver.as_str(), url = %redacted_url, pool_max = cfg.pool.max_size, pool_timeout_ms = cfg.pool.connect_timeout_ms, mig_on_start = %cfg.migrations_on_start.as_str(), "db config loaded");

        // Mobile policy: direct MySQL connections from a mobile app are not supported.
        // (This module is currently a mock pool scaffold; this guard prevents accidental
        // “it started but can never work” configurations on Android/iOS.)
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            if cfg.driver == DbDriverId::Mysql {
                anyhow::bail!(
                    "db.driver=mysql is not supported on mobile targets; use sqlite or move DB access to a server-side service"
                );
            }
        }

        // For now we do not implement real drivers here (those are in adapter crates).
        // We will expose a minimal in-process MockPool so the capability and registry wiring exists.
        // If url is missing, default to an in-memory sqlite-style URL for illustration.
        let _url = cfg.url.unwrap_or_else(|| "sqlite::memory:".to_string());

        // Build a simple mock pool that always returns a connection that pings OK.
        #[derive(Clone, Default)]
        struct MockPool;
        struct MockConn;
        impl DbConnection for MockConn {
            fn ping(&self) -> crate::Result<()> {
                Ok(())
            }
        }
        impl DbPool for MockPool {
            type Conn = MockConn;
            fn get(&self) -> crate::Result<Self::Conn> {
                Ok(MockConn)
            }
        }

        let pool = MockPool;
        // Try a quick readiness probe
        pool.get()
            .context("db: failed to acquire connection during init")?
            .ping()
            .context("db: ping failed during init")?;

        // Register in registry
        ctx.services
            .register::<DbHandle<MockPool>>(Arc::new(DbHandle(pool.clone())));
        info!(target = "airframe_db", "database pool registered");

        // If a Migrator was placed into the registry by the adapter or app, and migrations on_start is run, execute it.
        if let MigrationsMode::Run = cfg.migrations_on_start {
            if let Some(migrator) = ctx.services.get::<Arc<dyn Migrator + Send + Sync>>() {
                let conn = pool.get()?;
                // Choose a target version based on presence of migrations_path; for now, just ensure current_version runs.
                let _ = migrator.current_version(&conn)?;
                // We don't know target here; skip migrate_to to avoid side effects in default build.
            }
        }

        // Optional: integrate with cap:health if available
        if let Some(health) = airframe_health::ServiceRegistryHealthExt::health(&ctx.services) {
            let pool_clone = pool.clone();
            health.register_check("db", true, move |_cancel| {
                let pool = pool_clone.clone();
                async move {
                    match pool.get() {
                        Ok(conn) => match conn.ping() {
                            Ok(()) => airframe_health::HealthStatus::Healthy,
                            Err(e) => airframe_health::HealthStatus::Unhealthy(format!(
                                "ping failed: {:?}",
                                e
                            )),
                        },
                        Err(e) => airframe_health::HealthStatus::Unhealthy(format!(
                            "acquire failed: {:?}",
                            e
                        )),
                    }
                }
                .boxed()
            });
        }

        Ok(())
    }
}
