#![cfg(feature = "module")]

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures::FutureExt;
use tracing::{debug, info};

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_DB_PG};
use airframe_core::platform::PlatformSupport;
use airframe_core::registry::ServiceRegistry;
use airframe_macros::module_descriptor;

use airframe_config::api::types::BasicConfig;

use crate::{PgPool, PgPoolOptions};

/// Provides `cap:db.pg` by connecting a PostgreSQL pool from config and
/// registering it in the `ServiceRegistry`.
///
/// Config keys (toml section `[postgres]`):
/// - `postgres.url` — connection URL (default: `postgres://localhost:5432`)
/// - `postgres.pool_min` — minimum idle connections (default: 1)
/// - `postgres.pool_max` — maximum connections (default: 10)
/// - `postgres.pool_timeout_sec` — acquire timeout in seconds (default: 5)
///
/// Environment variable override:
/// - `POSTGRES_URL` — overrides `postgres.url` if set
///
/// Registered services:
/// - `Arc<PgPool>`
pub struct PgModule {
    desc: ModuleDescriptor,
}

impl PgModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "pg",
                version: "0.1.0",
                provides: [CAP_DB_PG.0],
            ),
        }
    }
}

/// Read PostgreSQL config from the `[postgres]` section of BasicConfig.
fn load_config(cfg: Option<Arc<BasicConfig>>) -> (String, u32, u32, u64) {
    let mut url = "postgres://localhost:5432".to_string();
    let mut pool_min: u32 = 1;
    let mut pool_max: u32 = 10;
    let mut pool_timeout_sec: u64 = 5;

    if let Some(bc) = cfg {
        if let Some(pg) = bc.raw.get("postgres") {
            if let Some(s) = pg.get("url").and_then(|v| v.as_str()) {
                url = s.to_string();
            }
            if let Some(v) = pg.get("pool_min").and_then(|v| v.as_integer()) {
                pool_min = v.max(0) as u32;
            }
            if let Some(v) = pg.get("pool_max").and_then(|v| v.as_integer()) {
                pool_max = v.max(1) as u32;
            }
            if let Some(v) = pg.get("pool_timeout_sec").and_then(|v| v.as_integer()) {
                pool_timeout_sec = v.max(1) as u64;
            }
        }
    }

    // Allow env override for url
    if let Ok(env_url) = std::env::var("POSTGRES_URL") {
        if !env_url.is_empty() {
            url = env_url;
        }
    }

    (url, pool_min, pool_max, pool_timeout_sec)
}

/// Mask the password portion of a database URL for logging.
fn mask_url(url: &str) -> String {
    if let Some(at_pos) = url.find('@') {
        if let Some(scheme_end) = url.find("://") {
            let user_pass = &url[scheme_end + 3..at_pos];
            if let Some(colon) = user_pass.find(':') {
                let user = &user_pass[..colon];
                return format!(
                    "{}://{}:***@{}",
                    &url[..scheme_end],
                    user,
                    &url[at_pos + 1..]
                );
            }
        }
    }
    url.to_string()
}

#[async_trait]
impl Module for PgModule {
    airframe_macros::impl_descriptor!();

    fn platform_support(&self) -> PlatformSupport {
        PlatformSupport::desktop_only(
            "pg module requires a PostgreSQL server and is not supported on mobile targets",
        )
    }

    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        let basic_cfg = ctx.services.get::<BasicConfig>();
        let (url, pool_min, pool_max, pool_timeout_sec) = load_config(basic_cfg);

        info!(
            target = "airframe_pg",
            url = %mask_url(&url),
            pool_min = pool_min,
            pool_max = pool_max,
            timeout_s = pool_timeout_sec,
            "postgres pool configured"
        );

        let opts = PgPoolOptions {
            min_connections: pool_min,
            max_connections: pool_max,
            connect_timeout_secs: pool_timeout_sec,
        };

        let pool = PgPool::connect(&url, opts)
            .await
            .map_err(|e| anyhow::anyhow!("postgres connect failed: {}", e))?;

        // Readiness probe
        pool.ping()
            .await
            .map_err(|e| anyhow::anyhow!("postgres ping failed: {}", e))?;

        debug!(
            target = "airframe_pg",
            "postgres pool connected and healthy"
        );

        let pool_arc = Arc::new(pool);
        ctx.services.register::<PgPool>(pool_arc.clone());

        // Integrate with health checks if available
        if let Some(health) = airframe_health::ServiceRegistryHealthExt::health(&ctx.services) {
            let pool_health = pool_arc.clone();
            health.register_check("postgres", true, move |_cancel| {
                let pool_health = pool_health.clone();
                async move {
                    match pool_health.ping().await {
                        Ok(()) => airframe_health::HealthStatus::Healthy,
                        Err(e) => airframe_health::HealthStatus::Unhealthy(format!(
                            "postgres ping failed: {}",
                            e
                        )),
                    }
                }
                .boxed()
            });
        }

        info!(
            target = "airframe_pg",
            "postgres pool registered in service registry"
        );
        Ok(())
    }
}

/// Convenience accessors for PgPool in the ServiceRegistry.
pub trait ServiceRegistryPgExt {
    fn pg_pool(&self) -> Option<Arc<PgPool>>;
}

impl ServiceRegistryPgExt for ServiceRegistry {
    fn pg_pool(&self) -> Option<Arc<PgPool>> {
        self.get::<PgPool>()
    }
}
