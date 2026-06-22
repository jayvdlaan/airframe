#![cfg(feature = "module")]

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use futures::FutureExt;
use semver::Version;
use tracing::{debug, error, info, warn};

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_CACHE_REDIS};
use airframe_core::platform::PlatformSupport;
use airframe_core::registry::ServiceRegistry;
use airframe_macros::module_descriptor;

use airframe_config::api::types::BasicConfig;

use crate::RedisByteCacheBuilder;

/// Provides cap:cache.redis by registering a Redis-backed ByteCache built from config.
///
/// Config keys (toml/env via airframe_config):
/// - redis.url = "redis://127.0.0.1/" (default)
/// - redis.namespace = "app" (default)
/// - redis.default_ttl_sec = 60 | null (optional)
///
/// Registered services:
/// - Arc<crate::RedisByteCache>
/// - Arc<dyn airframe_data::cache::ByteCache>
pub struct RedisModule {
    desc: ModuleDescriptor,
}

impl RedisModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "redis",
                version: "0.1.0",
                provides: [CAP_CACHE_REDIS.0]
            ),
        }
    }
}

fn parse_duration_secs(v: &toml::Value) -> Option<Duration> {
    v.as_integer().map(|i| {
        if i <= 0 {
            Duration::from_secs(0)
        } else {
            Duration::from_secs(i as u64)
        }
    })
}

/// Mask any `user:password@` credentials in a Redis URL so it is safe to log.
/// `redis://:secret@host:6379` becomes `redis://:***@host:6379`.
fn redact_url(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    let after = scheme_end + 3;
    let Some(at_rel) = url[after..].find('@') else {
        return url.to_string();
    };
    let at = after + at_rel;
    let creds = &url[after..at];
    let masked = match creds.split_once(':') {
        Some((user, _pass)) => format!("{user}:***"),
        None => creds.to_string(),
    };
    format!("{}{}@{}", &url[..after], masked, &url[at + 1..])
}

fn load_config(cfg: Option<Arc<BasicConfig>>) -> (String, String, Option<Duration>) {
    let mut url = "redis://127.0.0.1/".to_string();
    let mut namespace = "app".to_string();
    let mut ttl: Option<Duration> = None;

    if let Some(bc) = cfg {
        if let Some(redis) = bc.raw.get("redis") {
            if let Some(s) = redis.get("url").and_then(|v| v.as_str()) {
                url = s.to_string();
            }
            if let Some(s) = redis.get("namespace").and_then(|v| v.as_str()) {
                namespace = s.to_string();
            }
            if let Some(t) = redis.get("default_ttl_sec").and_then(parse_duration_secs) {
                // Treat 0 as no TTL
                if t.as_secs() > 0 {
                    ttl = Some(t);
                } else {
                    ttl = None;
                }
            }
        }
    }

    // Allow env override for url if set (useful without config module)
    if let Ok(env_url) = std::env::var("REDIS_URL") {
        if !env_url.is_empty() {
            url = env_url;
        }
    }

    (url, namespace, ttl)
}

#[async_trait]
impl Module for RedisModule {
    airframe_macros::impl_descriptor!();

    fn platform_support(&self) -> PlatformSupport {
        PlatformSupport::desktop_only(
            "redis module is intended for server-side deployments (external redis dependency) and is not supported on mobile targets",
        )
    }

    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        let basic_cfg = ctx.services.get::<BasicConfig>();
        let (url, ns, ttl) = load_config(basic_cfg);
        info!(target = "airframe_redis", url = %redact_url(&url), ns = %ns, ttl = ttl.as_ref().map(|d| d.as_secs()), "redis cache configured");

        // Build cache
        let cache = RedisByteCacheBuilder::new(url)
            .namespace(ns)
            .reuse_connection(true)
            .apply(|b| {
                if let Some(ttl) = ttl {
                    b.default_ttl(ttl)
                } else {
                    b
                }
            })
            .build()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        // Readiness probe: PING + tiny write/read/delete under namespace
        {
            let mut con = cache
                .clone()
                .client
                .get_connection()
                .map_err(|e| anyhow::anyhow!("redis connect failed: {}", e))?;
            let pong: String = redis::cmd("PING")
                .query(&mut con)
                .map_err(|e| anyhow::anyhow!("redis PING failed: {}", e))?;
            if pong.to_uppercase() != "PONG" {
                anyhow::bail!("unexpected PING response: {}", pong);
            }
        }
        // Write/read/delete probe using the ByteCache API
        {
            use airframe_data::cache::ByteCache as _; // trait in scope for methods
            let k = airframe_data::key::Key::new("__airframe_probe__").unwrap();
            cache.put_bytes(&k, b"ok")?;
            let got = cache.get_bytes(&k)?;
            anyhow::ensure!(
                got.as_deref() == Some(b"ok"),
                "redis probe readback mismatch"
            );
            let _ = cache.remove(&k);
        }

        let cache_arc = Arc::new(cache);
        // Register concrete cache type only (ByteCache is not object-safe)
        ctx.services
            .register::<crate::RedisByteCache>(cache_arc.clone());

        // Optional: integrate with cap:health if available
        if let Some(health) = airframe_health::ServiceRegistryHealthExt::health(&ctx.services) {
            let client = cache_arc.clone().client.clone();
            health.register_check("redis", true, move |_cancel| {
                let client = client.clone();
                async move {
                    match client.get_connection() {
                        Ok(mut con) => match redis::cmd("PING").query::<String>(&mut con) {
                            Ok(pong) if pong.to_uppercase() == "PONG" => {
                                airframe_health::HealthStatus::Healthy
                            }
                            Ok(other) => airframe_health::HealthStatus::Unhealthy(format!(
                                "unexpected PING: {}",
                                other
                            )),
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

/// Convenience accessors for redis-related services.
pub trait ServiceRegistryRedisExt {
    fn redis_byte_cache(&self) -> Option<Arc<crate::RedisByteCache>>;
}

impl ServiceRegistryRedisExt for ServiceRegistry {
    fn redis_byte_cache(&self) -> Option<Arc<crate::RedisByteCache>> {
        self.get::<crate::RedisByteCache>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airframe_core::app::AppBuilder;
    use airframe_data::cache::ByteCache; // trait methods
    use airframe_health::ServiceRegistryHealthExt; // extension trait for registry

    // Only run when REDIS_URL is available or local default likely works.
    fn should_run() -> bool {
        std::env::var("AIRFRAME_REDIS_TESTS").ok().is_some()
            || std::env::var("REDIS_URL").ok().is_some()
    }

    #[tokio::test]
    async fn module_registers_cache() {
        if !should_run() {
            return;
        }
        let app = AppBuilder::new()
            .with(airframe_health::HealthModule::new())
            .with(RedisModule::new())
            .start()
            .await
            .unwrap();
        // concrete
        let redis = app
            .services
            .redis_byte_cache()
            .expect("RedisByteCache present");
        // quick roundtrip using the concrete type
        let k = airframe_data::key::Key::new("modprobe").unwrap();
        ByteCache::put_bytes(&*redis, &k, b"1").unwrap();
        let got = ByteCache::get_bytes(&*redis, &k).unwrap().unwrap();
        assert_eq!(got, b"1");
        ByteCache::remove(&*redis, &k).unwrap();
        let list = ByteCache::list(&*redis).unwrap();
        assert!(list.into_iter().all(|kk| kk.as_str() != "modprobe"));
        // health check presence
        let health = app.services.health().expect("HealthService present");
        let names: Vec<String> = health
            .checks_snapshot()
            .into_iter()
            .map(|(n, _, _)| n)
            .collect();
        assert!(names.iter().any(|n| n == "redis"));

        // Health integration: if HealthService is present, the readiness barrier should pass
        if let Some(health) = airframe_health::ServiceRegistryHealthExt::health(&app.services) {
            tokio::time::timeout(std::time::Duration::from_secs(2), health.ready())
                .await
                .unwrap();
        }
    }
}
