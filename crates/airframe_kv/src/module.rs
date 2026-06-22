use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use tracing::{debug, info};

use airframe_core::bus::EventBus;
use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_KV};
use airframe_macros::module_descriptor;

use crate::acl::{AclMode, KvStoreAcl};
use crate::inmemory::InMemoryKvStore;
use crate::store::KvStore;

pub struct KvModule {
    desc: ModuleDescriptor,
    allow_prefixes: Option<Vec<String>>, // optional ACL for dyn KvStore
    acl_mode: AclMode,
}

impl Default for KvModule {
    fn default() -> Self {
        Self::new()
    }
}

impl KvModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "kv",
                version: "0.1.0",
                provides: [CAP_KV.0]
            ),
            allow_prefixes: None,
            acl_mode: AclMode::Warn,
        }
    }
    pub fn with_allow_prefixes(mut self, allow: Vec<String>, mode: AclMode) -> Self {
        self.allow_prefixes = Some(allow);
        self.acl_mode = mode;
        self
    }
}

#[async_trait]
impl Module for KvModule {
    airframe_macros::impl_descriptor!();
    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        // discover event bus if available
        let bus = ctx
            .services
            .get::<airframe_core::bus::inmem::InMemoryEventBus>();

        // Decide backend based on BasicConfig if present
        #[derive(Default, serde::Deserialize)]
        #[allow(dead_code)]
        struct KvFsCfg {
            root: Option<String>,
        }
        #[derive(Default, serde::Deserialize)]
        #[allow(dead_code)]
        struct KvCfg {
            backend: Option<String>,
            fs: Option<KvFsCfg>,
        }
        let cfg: Option<KvCfg> = {
            #[allow(unused_mut)]
            let mut out: Option<KvCfg> = None;
            #[cfg(feature = "airframe_config")]
            {
                if let Some(bc) = ctx
                    .services
                    .get::<airframe_config::api::types::BasicConfig>()
                {
                    out = bc.raw.get("kv").and_then(|v| v.clone().try_into().ok());
                }
            }
            out
        };
        let backend = cfg
            .as_ref()
            .and_then(|c| c.backend.as_deref())
            .unwrap_or("inmemory")
            .to_ascii_lowercase();

        // Mobile policy: filesystem-backed KV is not supported by default on Android/iOS.
        // Mobile apps need an explicit, app-private storage path and lifecycle-aware flushing.
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            if backend == "filesystem" {
                anyhow::bail!(
                    "kv.backend=filesystem is not supported on mobile targets; use inmemory or provide a mobile storage adapter"
                );
            }
        }

        // Build the selected store
        enum StoreArc {
            InMem(Arc<InMemoryKvStore>),
            #[allow(dead_code)]
            Dyn(Arc<dyn KvStore>),
        }
        let selected: StoreArc;
        match backend.as_str() {
            "filesystem" => {
                #[cfg(feature = "kv-fs")]
                {
                    let root = cfg
                        .as_ref()
                        .and_then(|c| c.fs.as_ref())
                        .and_then(|f| f.root.as_ref())
                        .cloned()
                        .unwrap_or_else(|| "./var/kv".to_string());
                    info!(target = "airframe_kv", backend = "filesystem", root = %root, "KV backend selected");
                    let fs = crate::filesystem::FilesystemKvStore::open(root).await?;
                    selected = StoreArc::Dyn(fs as Arc<dyn KvStore>);
                }
                #[cfg(not(feature = "kv-fs"))]
                {
                    // Fallback to in-memory with a warning
                    tracing::warn!(target = "airframe_kv", "filesystem backend requested but kv-fs feature not enabled; falling back to in-memory");
                    let store = InMemoryKvStore::new();
                    info!(
                        target = "airframe_kv",
                        backend = "inmemory",
                        "KV backend selected"
                    );
                    selected = StoreArc::InMem(Arc::new(store.clone()));
                }
            }
            _ => {
                let store = InMemoryKvStore::new();
                info!(
                    target = "airframe_kv",
                    backend = "inmemory",
                    "KV backend selected"
                );
                selected = StoreArc::InMem(Arc::new(store.clone()));
            }
        }

        // Forward events to EventBus if present (using trait-object watch_prefix)
        if let Some(bus) = bus {
            debug!(target = "airframe_kv", "forwarding KV events to EventBus");
            let dyn_kv: Arc<dyn KvStore> = match &selected {
                StoreArc::InMem(k) => k.clone() as Arc<dyn KvStore>,
                StoreArc::Dyn(d) => d.clone(),
            };
            // Create the watch stream before spawning to avoid missing initial events due to subscription races.
            let mut stream = dyn_kv.watch_prefix("")?;
            let bus_clone = bus.clone();
            let cancel = ctx.cancel.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => {
                            break;
                        }
                        evt = stream.next() => {
                            match evt {
                                Some(evt) => {
                                    let _ = bus_clone.publish(evt, None).await;
                                }
                                None => break,
                            }
                        }
                    }
                }
            });
        }

        // Register both a concrete store (when available) and the trait-object handle
        match selected {
            StoreArc::InMem(store_arc) => {
                ctx.services.register::<InMemoryKvStore>(store_arc.clone());
                let mut dyn_arc: Arc<dyn KvStore> = store_arc.clone();
                if let Some(allow) = &self.allow_prefixes {
                    dyn_arc = Arc::new(KvStoreAcl::new(
                        dyn_arc.clone(),
                        allow.clone(),
                        self.acl_mode.clone(),
                    ));
                }
                ctx.services.register::<dyn KvStore>(dyn_arc);
            }
            StoreArc::Dyn(store_arc) => {
                let mut dyn_arc: Arc<dyn KvStore> = store_arc.clone();
                if let Some(allow) = &self.allow_prefixes {
                    dyn_arc = Arc::new(KvStoreAcl::new(
                        dyn_arc.clone(),
                        allow.clone(),
                        self.acl_mode.clone(),
                    ));
                }
                ctx.services.register::<dyn KvStore>(dyn_arc);
            }
        }
        Ok(())
    }
}
