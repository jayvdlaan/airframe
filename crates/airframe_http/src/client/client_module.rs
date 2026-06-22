use std::sync::Arc;

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_HTTP_CLIENT};
use airframe_macros::module_descriptor;
use async_trait::async_trait;
use tracing::info;

/// Registers a reqwest-backed HttpClient into the ServiceRegistry.
pub struct ReqwestClientModule {
    desc: ModuleDescriptor,
}

impl ReqwestClientModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "http-client-reqwest",
                version: "0.1.0",
                provides: [CAP_HTTP_CLIENT.0]
            ),
        }
    }
}

#[async_trait]
impl Module for ReqwestClientModule {
    airframe_macros::impl_descriptor!();
    async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> {
        let client = crate::reqwest_client::ReqwestClient::new();
        ctx.services
            .register::<dyn crate::HttpClient<Error = reqwest::Error>>(Arc::new(client));
        info!(target = "airframe_http", "reqwest HttpClient registered");
        Ok(())
    }
}
