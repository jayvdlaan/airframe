//! Airframe runtime module wiring for codecs.
//!
//! This module is compiled only when feature = "module" is enabled.

use std::collections::HashMap;
use std::sync::Arc;

use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_CODEC, CAP_CONFIG};
use airframe_macros::module_descriptor;
use async_trait::async_trait;
use tracing::{debug, warn};

use crate::codecs::{BincodeCodec, CborCodec, JsonCodec};
use crate::error::AirframeCodecError;
use crate::Codec;

/// A concrete codec implementation wrapper used by the registry.
///
/// This is intentionally not trait-object based: the core [`crate::Codec`] trait is not object
/// safe due to its generic encode/decode methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecImpl {
    Cbor,
    Json,
    Bincode,
}

impl CodecImpl {
    pub const fn name(&self) -> &'static str {
        match self {
            CodecImpl::Cbor => CborCodec::NAME,
            CodecImpl::Json => JsonCodec::NAME,
            CodecImpl::Bincode => BincodeCodec::NAME,
        }
    }

    pub fn encode<T: serde::Serialize>(&self, t: &T) -> Result<Vec<u8>, AirframeCodecError> {
        match self {
            CodecImpl::Cbor => CborCodec.encode(t),
            CodecImpl::Json => JsonCodec.encode(t),
            CodecImpl::Bincode => BincodeCodec.encode(t),
        }
    }

    pub fn decode<T: serde::de::DeserializeOwned>(
        &self,
        bytes: &[u8],
    ) -> Result<T, AirframeCodecError> {
        match self {
            CodecImpl::Cbor => CborCodec.decode(bytes),
            CodecImpl::Json => JsonCodec.decode(bytes),
            CodecImpl::Bincode => BincodeCodec.decode(bytes),
        }
    }
}

/// Service: registry of available codecs by name.
#[derive(Debug, Clone, Default)]
pub struct CodecRegistry {
    codecs: HashMap<&'static str, CodecImpl>,
}

impl CodecRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, codec: CodecImpl) {
        self.codecs.insert(codec.name(), codec);
    }

    pub fn get(&self, name: &str) -> Option<CodecImpl> {
        self.codecs.get(name).copied()
    }

    pub fn names(&self) -> Vec<&'static str> {
        let mut v: Vec<&'static str> = self.codecs.keys().copied().collect();
        v.sort_unstable();
        v
    }
}

/// Service: the chosen default codec (config-driven).
#[derive(Debug, Clone, Copy)]
pub struct DefaultCodec {
    inner: CodecImpl,
}

impl DefaultCodec {
    pub fn new(inner: CodecImpl) -> Self {
        Self { inner }
    }

    pub fn name(&self) -> &'static str {
        self.inner.name()
    }

    pub fn encode<T: serde::Serialize>(&self, t: &T) -> Result<Vec<u8>, AirframeCodecError> {
        self.inner.encode(t)
    }

    pub fn decode<T: serde::de::DeserializeOwned>(
        &self,
        bytes: &[u8],
    ) -> Result<T, AirframeCodecError> {
        self.inner.decode(bytes)
    }
}

#[derive(Debug, Default, serde::Deserialize)]
struct CodecConfig {
    #[serde(default)]
    default: Option<String>,

    // Reserved for future policy controls.
    #[allow(dead_code)]
    #[serde(default)]
    envelope: Option<toml::Value>,
    #[allow(dead_code)]
    #[serde(default)]
    content_id: Option<String>,
}

fn select_default_codec_name(ctx: &ModuleContext) -> Option<String> {
    #[cfg(feature = "config")]
    {
        if let Some(cfg) = ctx
            .services
            .get::<airframe_config::api::types::BasicConfig>()
        {
            let cc: CodecConfig = cfg.get("codec");
            return cc.default;
        }
    }
    let _ = ctx;
    None
}

/// Airframe module that registers codec services.
pub struct CodecModule {
    desc: ModuleDescriptor,
}

impl Default for CodecModule {
    fn default() -> Self {
        Self::new()
    }
}

impl CodecModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "codec",
                version: "0.1.0",
                provides: [CAP_CODEC.0],
                optional_requires: [CAP_CONFIG.0]
            ),
        }
    }
}

#[async_trait]
impl Module for CodecModule {
    airframe_macros::impl_descriptor!();

    async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> {
        let mut reg = CodecRegistry::new();
        // Keep cbor available by default given current usage.
        reg.register(CodecImpl::Cbor);
        reg.register(CodecImpl::Json);
        reg.register(CodecImpl::Bincode);

        let requested = select_default_codec_name(&ctx);
        let chosen = requested
            .as_deref()
            .and_then(|n| reg.get(n))
            .unwrap_or(CodecImpl::Cbor);

        if let Some(req) = requested {
            if chosen.name() != req {
                warn!(
                    target = "airframe_codec",
                    requested = %req,
                    chosen = %chosen.name(),
                    available = ?reg.names(),
                    "unsupported codec.default; falling back"
                );
            } else {
                debug!(
                    target = "airframe_codec",
                    codec = %chosen.name(),
                    "default codec selected from config"
                );
            }
        }

        ctx.services.register::<CodecRegistry>(Arc::new(reg));
        ctx.services
            .register::<DefaultCodec>(Arc::new(DefaultCodec::new(chosen)));
        Ok(())
    }
}

#[cfg(all(test, feature = "config"))]
mod tests {
    use super::*;
    use airframe_core::module::Module;
    use airframe_core::registry::ServiceRegistry;
    use tokio_util::sync::CancellationToken;

    fn ctx_with_config(pairs: &[(&str, &str)]) -> ModuleContext {
        let services = ServiceRegistry::default();
        services.register::<airframe_config::api::types::BasicConfig>(Arc::new(
            airframe_config::api::types::BasicConfig::from_pairs(pairs),
        ));
        ModuleContext {
            services,
            cancel: CancellationToken::new(),
            span: tracing::Span::none(),
        }
    }

    #[tokio::test]
    async fn module_registers_registry_and_default_codec_fallback() {
        let mut m = CodecModule::new();
        let ctx = ctx_with_config(&[("codec.default", "does-not-exist")]);
        m.init(ctx.clone()).await.unwrap();

        let reg = ctx.services.get::<CodecRegistry>().expect("registry");
        assert!(reg.get("cbor").is_some());

        let def = ctx.services.get::<DefaultCodec>().expect("default codec");
        assert_eq!(def.name(), "cbor");
    }

    #[tokio::test]
    async fn module_selects_default_codec_from_config() {
        let mut m = CodecModule::new();
        let ctx = ctx_with_config(&[("codec.default", "json")]);
        m.init(ctx.clone()).await.unwrap();

        let def = ctx.services.get::<DefaultCodec>().expect("default codec");
        assert_eq!(def.name(), "json");
    }
}
