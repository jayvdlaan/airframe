use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_CRYPT, CAP_SECRETS};
use airframe_core::registry::ServiceRegistry;
use airframe_macros::module_descriptor;
use tracing::{debug, info};

use airframe_data::backend::mem::MemBackend;
use airframe_data::cache::BackendByteCache;

use airframe_crypt::suite::CipherSuite; // capability required
use airframe_crypt::sym::SymmetricAlgorithm;

use crate::SecretCache;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct SecretsConfig {
    #[serde(default)]
    key_id: Option<String>,
    #[serde(default)]
    cipher: Option<airframe_crypt::AlgorithmId>,
    #[serde(default)]
    cache: CacheConfig,
    #[serde(default)]
    key: SecretsKeyConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
enum SecretsBackendId {
    #[default]
    Mem,
    Fs,
    Redis,
    Winreg,
}
impl SecretsBackendId {
    #[allow(dead_code)]
    fn as_str(self) -> &'static str {
        match self {
            SecretsBackendId::Mem => "mem",
            SecretsBackendId::Fs => "fs",
            SecretsBackendId::Redis => "redis",
            SecretsBackendId::Winreg => "winreg",
        }
    }
    #[allow(dead_code)]
    fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "mem" => Some(SecretsBackendId::Mem),
            "fs" => Some(SecretsBackendId::Fs),
            "redis" => Some(SecretsBackendId::Redis),
            "winreg" => Some(SecretsBackendId::Winreg),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct CacheConfig {
    /// mem|fs|redis|winreg (for now only mem is implemented)
    #[serde(default)]
    backend: SecretsBackendId,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct SecretsKeyConfig {
    /// Optional hex-encoded raw symmetric key bytes (e.g., 64 hex chars for 32 bytes). DEV ONLY.
    #[serde(default)]
    bytes_hex: Option<String>,
}

fn select_cipher(aid: Option<airframe_crypt::AlgorithmId>) -> SymmetricAlgorithm {
    use core::convert::TryFrom;
    let aid = aid.unwrap_or(airframe_crypt::AlgorithmId::AesGcm);
    SymmetricAlgorithm::try_from(aid).unwrap_or(SymmetricAlgorithm::AesGcm)
}

fn parse_hex(s: &str) -> Result<Vec<u8>> {
    let s = s.trim();
    anyhow::ensure!(s.len().is_multiple_of(2), "hex length must be even");
    let bytes: Result<Vec<u8>, _> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect();
    bytes.context("invalid hex in secrets.key.bytes_hex")
}

/// Load and validate the secrets configuration from the service registry.
///
/// When the `config` feature is disabled or `BasicConfig` is not registered,
/// returns the default configuration. Fails early if a backend that is not yet
/// implemented is requested.
fn load_config(services: &ServiceRegistry) -> Result<SecretsConfig> {
    // Suppress unused-variable warning when `config` feature is disabled.
    let _ = services;

    #[cfg(feature = "config")]
    let cfg: SecretsConfig = {
        services
            .get::<airframe_config::BasicConfig>()
            .map(|bc| bc.get("secrets"))
            .unwrap_or_default()
    };
    #[cfg(not(feature = "config"))]
    let cfg: SecretsConfig = SecretsConfig::default();

    // Guard: configuration may declare backends that are not implemented yet.
    // Fail early with a clear error instead of silently falling back to mem.
    match cfg.cache.backend {
        SecretsBackendId::Mem => {}
        SecretsBackendId::Fs => {
            anyhow::bail!(
                "secrets.cache.backend=fs is not implemented yet; use mem (or implement filesystem-backed secrets)"
            );
        }
        SecretsBackendId::Redis => {
            anyhow::bail!(
                "secrets.cache.backend=redis is not implemented yet; use mem (or implement redis-backed secrets)"
            );
        }
        SecretsBackendId::Winreg => {
            anyhow::bail!(
                "secrets.cache.backend=winreg is not implemented yet; use mem (or implement winreg-backed secrets)"
            );
        }
    }

    Ok(cfg)
}

/// Build a default in-memory byte cache stack wrapped in `SecretCache`.
///
/// Future: select backend via `cfg.cache.backend`.
fn build_cache() -> SecretCache<BackendByteCache<MemBackend>> {
    let backend = MemBackend::new();
    let bytes = BackendByteCache::new(backend);
    let cache = SecretCache::new(bytes);
    info!(
        target = "airframe_secrets",
        backend = "mem",
        "SecretCache registered"
    );
    cache
}

/// Run a minimal encrypt/decrypt health probe to verify the crypto backend
/// works correctly with the configured cipher and key material.
fn run_crypto_probe(suite: &dyn CipherSuite, cfg: &SecretsConfig) -> Result<()> {
    let alg = select_cipher(cfg.cipher);
    let key: Vec<u8> = if let Some(hex) = cfg.key.bytes_hex.as_deref() {
        parse_hex(hex)?
    } else {
        vec![0u8; 32]
    };
    anyhow::ensure!(
        key.len() == 32,
        "secrets.key must be 32 bytes for current default algorithms"
    );
    let nonce = suite
        .random_bytes(12)
        .context("random nonce generation failed")?;
    let pt = b"probe";
    let ct = suite
        .sym_encrypt(alg, &key, &nonce, pt, None)
        .context("probe encrypt failed")?;
    let out = suite
        .sym_decrypt(alg, &key, &nonce, &ct, None)
        .context("probe decrypt failed")?;
    anyhow::ensure!(out == pt, "probe decrypt mismatch");
    debug!(target = "airframe_secrets", "probe encrypt/decrypt ok");
    Ok(())
}

/// Register an encrypt/decrypt health check with the health module, if available.
///
/// This is a best-effort integration: if `HealthService` is not registered, the
/// function is a no-op.
#[cfg(feature = "health")]
fn register_health_check(services: &ServiceRegistry) {
    use futures::FutureExt;

    if let Some(health) = airframe_health::ServiceRegistryHealthExt::health(services) {
        let suite = services.get::<airframe_crypt::suite::SoftwareCipherSuite>();
        health.register_check("secrets", true, move |_cancel| {
            let suite = suite.clone();
            async move {
                if let Some(suite) = suite {
                    let key = vec![0u8; 32];
                    let alg = SymmetricAlgorithm::AesGcm;
                    let nonce = suite.random_bytes(12);
                    match nonce {
                        Ok(n) => {
                            let pt = b"ok";
                            match suite.sym_encrypt(alg, &key, &n, pt, None) {
                                Ok(ct) => match suite.sym_decrypt(alg, &key, &n, &ct, None) {
                                    Ok(out) if out.as_slice() == pt => {
                                        airframe_health::HealthStatus::Healthy
                                    }
                                    Ok(_) => airframe_health::HealthStatus::Unhealthy(
                                        "decrypt mismatch".into(),
                                    ),
                                    Err(e) => airframe_health::HealthStatus::Unhealthy(format!(
                                        "decrypt error: {:?}",
                                        e
                                    )),
                                },
                                Err(e) => airframe_health::HealthStatus::Unhealthy(format!(
                                    "encrypt error: {:?}",
                                    e
                                )),
                            }
                        }
                        Err(e) => airframe_health::HealthStatus::Unhealthy(format!(
                            "nonce error: {:?}",
                            e
                        )),
                    }
                } else {
                    airframe_health::HealthStatus::Degraded("no SoftwareCipherSuite".into())
                }
            }
            .boxed()
        });
    }
}

/// Provides secrets by registering a default in-memory SecretCache.
///
/// Registered services:
/// - `Arc<SecretCache<BackendByteCache<MemBackend>>>`
///
/// Requires crypt so consumers can use the registered CipherSuite for
/// encrypt/decrypt operations with the cache. This module itself does not
/// perform long-running work.
pub struct SecretsModule {
    desc: ModuleDescriptor,
}

impl Default for SecretsModule {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretsModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "secrets",
                version: "0.1.0",
                provides: [CAP_SECRETS.0],
                requires: [CAP_CRYPT.0]
            ),
        }
    }
}

#[async_trait]
impl Module for SecretsModule {
    airframe_macros::impl_descriptor!();

    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        let suite = ctx
            .services
            .get::<dyn CipherSuite>()
            .unwrap_or_else(|| panic!("{} CipherSuite must be present", CAP_CRYPT.as_str()));

        let cfg = load_config(&ctx.services)?;

        let cache = build_cache();

        run_crypto_probe(&*suite, &cfg)?;

        ctx.services
            .register::<SecretCache<BackendByteCache<MemBackend>>>(Arc::new(cache));

        #[cfg(feature = "health")]
        register_health_check(&ctx.services);

        Ok(())
    }
}

/// Convenience accessors for secrets-related services.
pub trait ServiceRegistrySecretsExt {
    fn secrets_cache(&self) -> Option<Arc<SecretCache<BackendByteCache<MemBackend>>>>;
}

impl ServiceRegistrySecretsExt for ServiceRegistry {
    fn secrets_cache(&self) -> Option<Arc<SecretCache<BackendByteCache<MemBackend>>>> {
        self.get::<SecretCache<BackendByteCache<MemBackend>>>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airframe_core::app::AppBuilder;
    use airframe_crypt::sym::SymmetricAlgorithm;
    use airframe_data::key::Key;
    #[cfg(feature = "health")]
    use airframe_health::ServiceRegistryHealthExt;

    #[tokio::test]
    async fn registers_cache_and_roundtrip() {
        let app = AppBuilder::new()
            .with(airframe_crypt::CryptModule::new())
            .with(SecretsModule::new())
            .start()
            .await
            .unwrap();

        let cache = app.services.secrets_cache().expect("SecretCache present");
        let suite = app
            .services
            .get::<airframe_crypt::suite::SoftwareCipherSuite>()
            .unwrap();

        // Simple typed roundtrip
        #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
        struct Demo {
            a: u32,
            b: String,
        }
        let k = Key::new("demo:1").unwrap();
        let v = Demo {
            a: 1,
            b: "x".into(),
        };
        let key = crate::SecretBytes::from_vec(vec![9u8; 32]);

        cache
            .put_value(
                &k,
                &*suite,
                SymmetricAlgorithm::ChaCha20Poly1305,
                &key,
                &v,
                None,
            )
            .unwrap();
        let out: Demo = cache.get_value(&k, &*suite, &key, None).unwrap().unwrap();
        assert_eq!(out, v);
    }

    #[cfg(feature = "health")]
    #[tokio::test]
    async fn registers_health_check_when_healthmodule_present() {
        let app = AppBuilder::new()
            .with(airframe_crypt::CryptModule::new())
            .with(airframe_health::HealthModule::new())
            .with(SecretsModule::new())
            .start()
            .await
            .unwrap();
        let health = app.services.health().expect("HealthService present");
        let names: Vec<String> = health
            .checks_snapshot()
            .into_iter()
            .map(|(n, _, _)| n)
            .collect();
        assert!(names.iter().any(|n| n == "secrets"));
    }
}
