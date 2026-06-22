// Compiled only with feature = "module" (gated at the `mod module;` site in lib.rs).

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures::FutureExt;
use tracing::info;

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_DB};
use airframe_core::platform::PlatformSupport;
use airframe_core::registry::ServiceRegistry;
use airframe_macros::module_descriptor;

use airframe_config::api::types::BasicConfig;

use crate::MySqlPool;

/// Provides cap:db by registering a MySQL-backed connection pool built from config.
///
/// Config keys (toml/env via airframe_config):
/// - mysql.url = "mysql://root:@localhost:3306/" (default)
/// - mysql.database = "" (optional, database/schema to USE after connect)
///
/// Registered services:
/// - Arc<crate::MySqlPool>
pub struct MySqlModule {
    desc: ModuleDescriptor,
}

impl Default for MySqlModule {
    fn default() -> Self {
        Self::new()
    }
}

impl MySqlModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "mysql",
                version: "0.1.0",
                provides: [CAP_DB.0],
            ),
        }
    }
}

fn load_config(cfg: Option<Arc<BasicConfig>>) -> (String, Option<String>) {
    let mut url = "mysql://root:@localhost:3306/".to_string();
    let mut database: Option<String> = None;

    if let Some(bc) = cfg {
        if let Some(mysql) = bc.raw.get("mysql") {
            if let Some(s) = mysql.get("url").and_then(|v| v.as_str()) {
                url = s.to_string();
            }
            if let Some(s) = mysql.get("database").and_then(|v| v.as_str()) {
                if !s.is_empty() {
                    database = Some(s.to_string());
                }
            }
        }
    }

    // Allow env override for url if set (useful without config module)
    if let Ok(env_url) = std::env::var("MYSQL_URL") {
        if !env_url.is_empty() {
            url = env_url;
        }
    }
    if let Ok(env_db) = std::env::var("MYSQL_DATABASE") {
        if !env_db.is_empty() {
            database = Some(env_db);
        }
    }

    (url, database)
}

#[async_trait]
impl Module for MySqlModule {
    airframe_macros::impl_descriptor!();

    fn platform_support(&self) -> PlatformSupport {
        PlatformSupport::desktop_only(
            "mysql module is intended for server-side deployments (external mysql dependency) and is not supported on mobile targets",
        )
    }

    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        let basic_cfg = ctx.services.get::<BasicConfig>();
        let (url, database) = load_config(basic_cfg);
        info!(target = "airframe_mysql", url = %url, database = database.as_deref().unwrap_or("(none)"), "mysql pool configured");

        // Build pool
        let pool = match &database {
            Some(db) => MySqlPool::with_db(&url, db),
            None => MySqlPool::new(&url),
        };

        // Readiness probe: open a connection and ping
        {
            use airframe_db::connection::{DbConnection as _, DbPool as _};
            let conn = pool
                .get()
                .map_err(|e| anyhow::anyhow!("mysql connect failed: {}", e))?;
            conn.ping()
                .map_err(|e| anyhow::anyhow!("mysql PING failed: {}", e))?;
        }

        let pool_arc = Arc::new(pool);
        ctx.services.register::<MySqlPool>(pool_arc.clone());

        // Optional: integrate with cap:health if available
        if let Some(health) = airframe_health::ServiceRegistryHealthExt::health(&ctx.services) {
            let pool_clone = pool_arc.clone();
            health.register_check("mysql", true, move |_cancel| {
                let pool_clone = pool_clone.clone();
                async move {
                    use airframe_db::connection::{DbConnection as _, DbPool as _};
                    match pool_clone.get() {
                        Ok(conn) => match conn.ping() {
                            Ok(()) => airframe_health::HealthStatus::Healthy,
                            Err(e) => airframe_health::HealthStatus::Unhealthy(format!(
                                "PING failed: {}",
                                e
                            )),
                        },
                        Err(e) => airframe_health::HealthStatus::Unhealthy(format!(
                            "connect failed: {}",
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

/// Convenience accessors for mysql-related services.
pub trait ServiceRegistryMySqlExt {
    fn mysql_pool(&self) -> Option<Arc<MySqlPool>>;
}

impl ServiceRegistryMySqlExt for ServiceRegistry {
    fn mysql_pool(&self) -> Option<Arc<MySqlPool>> {
        self.get::<MySqlPool>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airframe_core::app::AppBuilder;
    use airframe_db::connection::DbPool;
    use airframe_health::ServiceRegistryHealthExt;

    // Only run when MYSQL_URL is available or local default likely works.
    fn should_run() -> bool {
        std::env::var("AIRFRAME_MYSQL_TESTS").ok().is_some()
            || std::env::var("MYSQL_URL").ok().is_some()
    }

    #[tokio::test]
    async fn module_registers_pool() {
        if !should_run() {
            return;
        }
        let app = AppBuilder::new()
            .with(airframe_health::HealthModule::new())
            .with(MySqlModule::new())
            .start()
            .await
            .unwrap();
        // concrete
        let pool = app.services.mysql_pool().expect("MySqlPool present");
        // quick ping using the pool
        let conn = pool.get().expect("get connection");
        use airframe_db::connection::DbConnection as _;
        conn.ping().expect("ping ok");

        // health check presence
        let health = app.services.health().expect("HealthService present");
        let names: Vec<String> = health
            .checks_snapshot()
            .into_iter()
            .map(|(n, _, _)| n)
            .collect();
        assert!(names.iter().any(|n| n == "mysql"));

        // Health integration: if HealthService is present, the readiness barrier should pass
        if let Some(health) = airframe_health::ServiceRegistryHealthExt::health(&app.services) {
            tokio::time::timeout(std::time::Duration::from_secs(2), health.ready())
                .await
                .unwrap();
        }
    }
}
