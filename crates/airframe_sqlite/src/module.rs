use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::FutureExt;
use tracing::{debug, info};

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_DB_SQLITE};
use airframe_core::registry::ServiceRegistry;
use airframe_macros::module_descriptor;

use airframe_config::api::types::BasicConfig;
use airframe_db::connection::{DbConnection, DbPool};

use crate::SqlitePool;

/// Provides cap:db.sqlite by registering a SQLite pool built from config.
///
/// Config keys (toml/env via airframe_config):
/// - sqlite.path = ":memory:" (default)
/// - sqlite.pragmas = ["PRAGMA journal_mode=WAL", ...] (optional)
///
/// Registered services:
/// - Arc<crate::SqlitePool>
pub struct SqliteModule {
    desc: ModuleDescriptor,
}

impl Default for SqliteModule {
    fn default() -> Self {
        Self::new()
    }
}

impl SqliteModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "sqlite",
                version: "0.1.0",
                provides: [CAP_DB_SQLITE.0]
            ),
        }
    }
}

fn load_config(cfg: Option<Arc<BasicConfig>>) -> (String, Vec<String>) {
    let mut path = ":memory:".to_string();
    let mut pragmas: Vec<String> = Vec::new();

    if let Some(bc) = cfg {
        if let Some(sqlite) = bc.raw.get("sqlite") {
            if let Some(s) = sqlite.get("path").and_then(|v| v.as_str()) {
                path = s.to_string();
            }
            if let Some(arr) = sqlite.get("pragmas").and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(s) = item.as_str() {
                        pragmas.push(s.to_string());
                    }
                }
            }
        }
    }

    // Allow env override for path if set (useful without config module)
    if let Ok(env_path) = std::env::var("SQLITE_PATH") {
        if !env_path.is_empty() {
            path = env_path;
        }
    }

    (path, pragmas)
}

#[async_trait]
impl Module for SqliteModule {
    airframe_macros::impl_descriptor!();

    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        let basic_cfg = ctx.services.get::<BasicConfig>();
        let (path, pragmas) = load_config(basic_cfg);
        info!(target = "airframe_sqlite", path = %path, pragmas = ?pragmas, "sqlite configured");

        // Build pool
        let pool = SqlitePool::new(&path).with_pragmas(pragmas);

        // Readiness probe: acquire a connection and ping
        {
            let conn = pool
                .get()
                .context("sqlite: failed to acquire connection during init")?;
            conn.ping().context("sqlite: ping failed during init")?;
        }
        debug!(target = "airframe_sqlite", "sqlite readiness probe passed");

        let pool_arc = Arc::new(pool);
        ctx.services.register::<SqlitePool>(pool_arc.clone());
        info!(target = "airframe_sqlite", "sqlite pool registered");

        // Optional: integrate with cap:health if available
        if let Some(health) = airframe_health::ServiceRegistryHealthExt::health(&ctx.services) {
            let pool_health = pool_arc.clone();
            health.register_check("sqlite", true, move |_cancel| {
                let pool = pool_health.clone();
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

/// Convenience accessors for sqlite-related services.
pub trait ServiceRegistrySqliteExt {
    fn sqlite_pool(&self) -> Option<Arc<SqlitePool>>;
}

impl ServiceRegistrySqliteExt for ServiceRegistry {
    fn sqlite_pool(&self) -> Option<Arc<SqlitePool>> {
        self.get::<SqlitePool>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airframe_core::app::AppBuilder;
    use airframe_health::ServiceRegistryHealthExt;

    #[tokio::test]
    async fn module_registers_pool() {
        let app = AppBuilder::new()
            .with(airframe_health::HealthModule::new())
            .with(SqliteModule::new())
            .start()
            .await
            .unwrap();

        // Retrieve the registered pool
        let pool = app.services.sqlite_pool().expect("SqlitePool present");

        // Verify pool is functional: acquire connection and ping
        use airframe_db::connection::DbConnection;
        let conn = pool.get().unwrap();
        conn.ping().unwrap();

        // Health check presence
        let health = app.services.health().expect("HealthService present");
        let names: Vec<String> = health
            .checks_snapshot()
            .into_iter()
            .map(|(n, _, _)| n)
            .collect();
        assert!(names.iter().any(|n| n == "sqlite"));
    }
}
