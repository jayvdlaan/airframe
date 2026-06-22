use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::store::{KvEvent, KvMetadata, KvStore, KvStoreExt};

// Helper function for typed watch on the trait-object path
pub fn kv_watch_prefix_t<T: serde::de::DeserializeOwned + Send + 'static>(
    kv: Arc<dyn KvStore>,
    prefix: &str,
) -> Result<ReceiverStream<(String, T, KvMetadata)>> {
    let mut evts = kv.watch_prefix(prefix)?;
    let pref = prefix.to_string();
    let kv2 = kv.clone();
    let (out_tx, out_rx) = mpsc::channel(1024);
    tokio::spawn(async move {
        use futures::StreamExt;
        while let Some(evt) = evts.next().await {
            if let KvEvent::Put { key, .. } = evt {
                if key.starts_with(&pref) {
                    if let Ok(Some((val, meta))) = KvStoreExt::get_t::<T>(&*kv2, &key).await {
                        if out_tx.send((key, val, meta)).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    });
    Ok(ReceiverStream::new(out_rx))
}

// PrefixEvent for typed watch with deletes/expire
#[derive(Debug, Clone)]
pub enum PrefixEvent<T> {
    Put {
        key: String,
        value: T,
        meta: KvMetadata,
    },
    Delete {
        key: String,
        meta: Option<KvMetadata>,
    },
    Expire {
        key: String,
        meta: Option<KvMetadata>,
    },
}

pub fn kv_watch_prefix_t_with_deletes<T: serde::de::DeserializeOwned + Send + 'static>(
    kv: Arc<dyn KvStore>,
    prefix: &str,
) -> Result<ReceiverStream<PrefixEvent<T>>> {
    let mut evts = kv.watch_prefix(prefix)?;
    let pref = prefix.to_string();
    let kv2 = kv.clone();
    let (out_tx, out_rx) = mpsc::channel(1024);
    tokio::spawn(async move {
        use futures::StreamExt;
        use std::collections::HashMap;
        let mut cache: HashMap<String, KvMetadata> = HashMap::new();
        while let Some(evt) = evts.next().await {
            match evt {
                KvEvent::Put { key, .. } => {
                    if key.starts_with(&pref) {
                        if let Ok(Some((val, meta))) = KvStoreExt::get_t::<T>(&*kv2, &key).await {
                            cache.insert(key.clone(), meta.clone());
                            if out_tx
                                .send(PrefixEvent::Put {
                                    key,
                                    value: val,
                                    meta,
                                })
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
                KvEvent::Delete { key } => {
                    if key.starts_with(&pref) {
                        let meta = cache.get(&key).cloned();
                        if out_tx
                            .send(PrefixEvent::Delete { key, meta })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
                KvEvent::Expire { key } => {
                    if key.starts_with(&pref) {
                        let meta = cache.get(&key).cloned();
                        if out_tx
                            .send(PrefixEvent::Expire { key, meta })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        }
    });
    Ok(ReceiverStream::new(out_rx))
}
