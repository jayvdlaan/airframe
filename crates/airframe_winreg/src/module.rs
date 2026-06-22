//! Airframe runtime module for Windows Registry-backed ByteCache.
//!
//! Provides the Windows Registry cache capability and registers a WinRegByteCache
//! into the ServiceRegistry for consumers. Windows-only.

#![cfg(target_os = "windows")]

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures::FutureExt;
use semver::Version;
use tracing::{debug, info, warn};

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_CACHE_WINREG};
use airframe_core::registry::ServiceRegistry;
use airframe_data::cache::ByteCache;
use airframe_macros::module_descriptor;

use crate::{HiveKind, WinRegByteCache};

/// Configuration namespace keys and defaults
///
/// winreg.hive = "HKCU" | "HKLM"
/// winreg.path = "Software\\Airframe\\Cache"
#[derive(Clone, Debug)]
struct WinRegConfig {
    hive: HiveKind,
    path: String,
}

#[cfg(feature = "config")]
fn load_config(bc: Option<Arc<airframe_config::api::types::BasicConfig>>) -> WinRegConfig {
    let mut hive = HiveKind::CurrentUser;
    let mut path = r"Software\Airframe\Cache".to_string();
    if let Some(cfg) = bc {
        let raw = &cfg.raw;
        if let Some(h) = raw
            .get("winreg")
            .and_then(|w| w.get("hive"))
            .and_then(|v| v.as_str())
        {
            match h.to_ascii_uppercase().as_str() {
                "HKCU" | "CURRENTUSER" | "CURRENT_USER" => hive = HiveKind::CurrentUser,
                "HKLM" | "LOCALMACHINE" | "LOCAL_MACHINE" => hive = HiveKind::LocalMachine,
                _ => {}
            }
        }
        if let Some(p) = raw
            .get("winreg")
            .and_then(|w| w.get("path"))
            .and_then(|v| v.as_str())
        {
            if !p.is_empty() {
                path = p.to_string();
            }
        }
    }
    WinRegConfig { hive, path }
}

#[cfg(not(feature = "config"))]
fn load_config(_bc: Option<Arc<()>>) -> WinRegConfig {
    // Without the config feature, fall back to defaults
    WinRegConfig {
        hive: HiveKind::CurrentUser,
        path: r"Software\Airframe\Cache".to_string(),
    }
}

/// Windows Registry cache module.
pub struct WinRegModule {
    desc: ModuleDescriptor,
}

impl WinRegModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "winreg",
                version: "0.1.0",
                provides: [CAP_CACHE_WINREG.0]
            ),
        }
    }
}

#[async_trait]
impl Module for WinRegModule {
    airframe_macros::impl_descriptor!();

    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        // Prefer reading from BasicConfig if present; otherwise use defaults
        #[cfg(feature = "config")]
        let cfg = load_config(
            ctx.services
                .get::<airframe_config::api::types::BasicConfig>(),
        );
        #[cfg(not(feature = "config"))]
        let cfg = load_config(None);
        info!(target = "airframe_winreg", hive = ?cfg.hive, path = %cfg.path, "WinReg cache configured");
        let cache = Arc::new(WinRegByteCache::new(cfg.hive, cfg.path));

        // Readiness probe: attempt write+read+delete of a temp key to ensure access
        let probe_key = airframe_data::key::Key::new("__airframe_probe__").unwrap();
        let probe_bytes = b"ok";
        ByteCache::put_bytes(&*cache, &probe_key, probe_bytes)?;
        let got = ByteCache::get_bytes(&*cache, &probe_key)?;
        if got.as_deref() != Some(probe_bytes) {
            anyhow::bail!("winreg probe readback mismatch");
        }
        let _ = ByteCache::remove(&*cache, &probe_key);

        // Register both the concrete cache and trait-object for consumers
        ctx.services.register::<WinRegByteCache>(cache.clone());
        let dyn_bc: Arc<dyn ByteCache> = cache.clone();
        ctx.services.register::<dyn ByteCache>(dyn_bc);

        // Optional: integrate with cap:health if available (simple write/read check).
        // Gated behind the `health` feature so airframe_winreg does not unconditionally
        // pull in airframe_health (it was the only non-optional lateral L4 edge).
        #[cfg(feature = "health")]
        if let Some(health) = airframe_health::ServiceRegistryHealthExt::health(&ctx.services) {
            let cache_clone = cache.clone();
            health.register_check("winreg", true, move |_cancel| {
                let cache = cache_clone.clone();
                async move {
                    let key = airframe_data::key::Key::new("__health__").unwrap();
                    match ByteCache::put_bytes(&*cache, &key, b"1") {
                        Ok(()) => match ByteCache::get_bytes(&*cache, &key) {
                            Ok(Some(ref v)) if v == b"1" => {
                                let _ = ByteCache::remove(&*cache, &key);
                                airframe_health::HealthStatus::Healthy
                            }
                            Ok(_) => {
                                airframe_health::HealthStatus::Unhealthy("readback mismatch".into())
                            }
                            Err(_) => airframe_health::HealthStatus::Unhealthy("get failed".into()),
                        },
                        Err(_) => airframe_health::HealthStatus::Unhealthy("put failed".into()),
                    }
                }
                .boxed()
            });
        }
        Ok(())
    }
}

/// Convenience accessors for ServiceRegistry
pub trait ServiceRegistryWinRegExt {
    fn winreg_cache(&self) -> Option<Arc<WinRegByteCache>>;
    fn bytecache_dyn(&self) -> Option<Arc<dyn ByteCache>>;
}
impl ServiceRegistryWinRegExt for ServiceRegistry {
    fn winreg_cache(&self) -> Option<Arc<WinRegByteCache>> {
        self.get::<WinRegByteCache>()
    }
    fn bytecache_dyn(&self) -> Option<Arc<dyn ByteCache>> {
        self.get::<dyn ByteCache>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "config")]
    use airframe_config::ConfigModule;
    use airframe_core::app::AppBuilder;

    #[cfg(all(feature = "config", feature = "health"))]
    #[tokio::test]
    async fn registers_and_probes() {
        // Provide a minimal config with a test path under HKCU
        let defaults = toml::toml! {
            [winreg]
            hive = "HKCU"
            path = "Software\\Airframe\\TestCache"
        };
        let app = AppBuilder::new()
            .with(ConfigModule::new(None).with_defaults(defaults))
            .with(airframe_health::HealthModule::new())
            .with(WinRegModule::new())
            .start()
            .await
            .unwrap();
        // concrete
        let conc = app.services.winreg_cache().expect("concrete present");
        // dyn trait-object
        let bc = app
            .services
            .get::<dyn ByteCache>()
            .expect("dyn ByteCache present");
        // simple put/get/remove to ensure it works
        let k = airframe_data::key::Key::new("it_works").unwrap();
        ByteCache::put_bytes(&*bc, &k, b"1").unwrap();
        let got = ByteCache::get_bytes(&*bc, &k).unwrap().unwrap();
        assert_eq!(got, b"1");
        ByteCache::remove(&*bc, &k).unwrap();
        let listed = conc.list().unwrap();
        assert!(listed.into_iter().all(|kk| kk.as_str() != "it_works"));
        // health check presence
        let health = app.services.health().expect("HealthService present");
        let names: Vec<String> = health
            .checks_snapshot()
            .into_iter()
            .map(|(n, _, _)| n)
            .collect();
        assert!(names.iter().any(|n| n == "winreg"));
    }
}
