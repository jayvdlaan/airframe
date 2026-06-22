use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_AUDIT};
use airframe_core::registry::ServiceRegistry;
use airframe_macros::module_descriptor;
use tracing::info;

use crate::chain::{AuditChain, AuditChainConfig};
use crate::crypto::AuditCrypto;
use crate::store::AuditStore;

/// Airframe module that wires up the audit chain from pre-registered services.
///
/// If `AuditCrypto` and `AuditStore` are already registered in the `ServiceRegistry`,
/// this module creates an `AuditChain` and registers it. Otherwise it is a no-op
/// (the application is expected to wire it manually).
pub struct AuditModule {
    desc: ModuleDescriptor,
}

impl Default for AuditModule {
    fn default() -> Self {
        Self::new()
    }
}

impl AuditModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "airframe_audit",
                version: "0.1.0",
                provides: [CAP_AUDIT.0]
            ),
        }
    }
}

#[async_trait]
impl Module for AuditModule {
    airframe_macros::impl_descriptor!();

    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        if let (Some(crypto), Some(store)) = (
            ctx.services.get::<dyn AuditCrypto>(),
            ctx.services.get::<dyn AuditStore>(),
        ) {
            let config = ctx
                .services
                .get::<AuditChainConfig>()
                .map(|c| (*c).clone())
                .unwrap_or_default();
            info!(target: "airframe_audit", "registering audit chain");
            let chain = Arc::new(AuditChain::new(crypto, store, config));
            ctx.services.register::<AuditChain>(chain);
        } else {
            info!(target: "airframe_audit", "no AuditCrypto/AuditStore pre-registered; skipping");
        }
        Ok(())
    }
}

/// Extension trait for `ServiceRegistry` to access the audit chain.
pub trait ServiceRegistryAuditExt {
    fn audit_chain(&self) -> Option<Arc<AuditChain>>;
}

impl ServiceRegistryAuditExt for ServiceRegistry {
    fn audit_chain(&self) -> Option<Arc<AuditChain>> {
        self.get::<AuditChain>()
    }
}
