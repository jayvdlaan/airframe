use async_trait::async_trait;
use semver::Version;
use tokio_util::sync::CancellationToken;
use tracing::Span;

use crate::platform::PlatformSupport;
use crate::registry::ServiceRegistry;

/// Typed capability identifier. Wraps a &'static str label like "cap:http.server".
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Cap(pub &'static str);

impl Cap {
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        self.0
    }
}

/// Helper to declare a versioned capability requirement tuple using a typed Cap.
#[inline]
pub const fn cap_req(cap: Cap, range: &'static str) -> (&'static str, &'static str) {
    (cap.0, range)
}

// Common capability constants used across Airframe crates
pub const CAP_HTTP_SERVER: Cap = Cap("cap:http.server");
pub const CAP_HTTP_CLIENT: Cap = Cap("cap:http.client");
pub const CAP_CONFIG: Cap = Cap("cap:config");
pub const CAP_CODEC: Cap = Cap("cap:codec");
pub const CAP_LOGGING: Cap = Cap("cap:logging");
pub const CAP_HEALTH: Cap = Cap("cap:health");
pub const CAP_ARGS: Cap = Cap("cap:args");
pub const CAP_KV: Cap = Cap("cap:kv");
pub const CAP_DB: Cap = Cap("cap:db");
pub const CAP_DB_PG: Cap = Cap("cap:db.pg");
pub const CAP_DB_SQLITE: Cap = Cap("cap:db.sqlite");
pub const CAP_CRYPT: Cap = Cap("cap:crypt");
pub const CAP_SECRETS: Cap = Cap("cap:secrets");
pub const CAP_PDATA: Cap = Cap("cap:pdata");
pub const CAP_SDATA: Cap = Cap("cap:sdata");
pub const CAP_SCHEDULER: Cap = Cap("cap:scheduler");
pub const CAP_ROUTER: Cap = Cap("cap:router");
pub const CAP_CACHE_REDIS: Cap = Cap("cap:cache.redis");
pub const CAP_CACHE_WINREG: Cap = Cap("cap:cache.winreg");
// Additional well-known framework capabilities to improve discoverability
pub const CAP_GATEWAY: Cap = Cap("cap:gateway");
pub const CAP_WORKER: Cap = Cap("cap:worker");
pub const CAP_HTTP_ROUTER_ADMIN: Cap = Cap("cap:http.router.admin");
pub const CAP_OPENAPI: Cap = Cap("cap:openapi");
pub const CAP_METRICS: Cap = Cap("cap:metrics");

// NOTE: capabilities owned by a higher-layer super-project are defined by THAT
// project, not enumerated here — L0 core does not carry app/extension
// vocabulary. They are declared (today as raw strings) where they are provided:
//   airframe-srv:     cap:sessions, cap:secret-cache, cap:seal-state, cap:mac,
//                     cap:gate, cap:replay, cap:middleware, cap:migrations
//   airframe-cryptex: cap:smartcard, cap:piv
//   airframe-svc:     cap:signals, cap:shutdown, cap:supervisor
//   airframe-app:     cap:tauri

// Internal test-only placeholder capabilities: compiled only in airframe_core's
// own test builds and kept out of the public capability vocabulary.
#[cfg(test)]
pub(crate) const CAP_A: Cap = Cap("cap:a");
#[cfg(test)]
pub(crate) const CAP_X: Cap = Cap("cap:x");
#[cfg(test)]
pub(crate) const CAP_Y: Cap = Cap("cap:y");
#[cfg(test)]
pub(crate) const CAP_B: Cap = Cap("cap:b");

// Support capabilities consumed by downstream examples / integration tests.
pub const CAP_CLI_ADMIN: Cap = Cap("cap:cli.admin");
pub const CAP_EXAMPLE_API: Cap = Cap("cap:example.api");
pub const CAP_TEST_API: Cap = Cap("cap:test.api");
pub const CAP_AUDIT: Cap = Cap("cap:audit");

#[derive(Clone, Debug)]
pub struct ModuleDescriptor {
    pub name: &'static str,
    pub version: Version,
    pub provides: &'static [&'static str],
    pub requires: &'static [&'static str],
    pub optional_requires: &'static [&'static str],
    /// Optional versioned capability requirements as (capability, semver range)
    pub requires_with_versions: &'static [(&'static str, &'static str)],
    pub optional_requires_with_versions: &'static [(&'static str, &'static str)],
}

#[derive(Clone)]
pub struct ModuleContext {
    pub services: ServiceRegistry,
    // NOTE: Event/Command/Query buses will be added later once object-safe API surface is finalized.
    pub cancel: CancellationToken,
    pub span: Span,
}

#[async_trait]
pub trait Module: Send {
    fn descriptor(&self) -> &ModuleDescriptor;

    /// Declares platform support for this module.
    ///
    /// Default is "supported everywhere"; modules that are OS-specific or not yet
    /// supported on certain targets should override this and return a narrower
    /// [`PlatformSupport`].
    fn platform_support(&self) -> PlatformSupport {
        PlatformSupport::all()
    }

    async fn init(&mut self, _ctx: ModuleContext) -> anyhow::Result<()> {
        Ok(())
    }
    async fn start(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn stop(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyModule {
        desc: ModuleDescriptor,
        inited: bool,
        started: bool,
        stopped: bool,
    }

    #[async_trait]
    impl Module for DummyModule {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, _ctx: ModuleContext) -> anyhow::Result<()> {
            self.inited = true;
            Ok(())
        }
        async fn start(&mut self) -> anyhow::Result<()> {
            self.started = true;
            Ok(())
        }
        async fn stop(&mut self) -> anyhow::Result<()> {
            self.stopped = true;
            Ok(())
        }
    }

    #[tokio::test]
    async fn dummy_module_lifecycle() {
        let desc = ModuleDescriptor {
            name: "dummy",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        };
        let mut m = DummyModule {
            desc,
            inited: false,
            started: false,
            stopped: false,
        };

        let ctx = ModuleContext {
            services: ServiceRegistry::default(),
            cancel: CancellationToken::new(),
            span: tracing::Span::none(),
        };

        assert_eq!(m.descriptor().name, "dummy");
        m.init(ctx.clone()).await.unwrap();
        m.start().await.unwrap();
        m.stop().await.unwrap();
        assert!(m.inited && m.started && m.stopped);
    }
}
