use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use dashmap::DashMap;
use futures::StreamExt;
use tokio::{sync::mpsc, time};
use tokio_stream::wrappers::ReceiverStream;

use spacetime_core as st;

use crate::store::{DeleteResult, KvEvent, KvMetadata, KvStore, Page, PutOptions, PutResult};
use crate::watch::PrefixEvent;

#[derive(Clone)]
struct Entry {
    value: Vec<u8>,
    meta: KvMetadata,
    expire_at: Option<st::Instant>,
}

#[derive(Clone)]
pub struct InMemoryKvStore {
    map: Arc<DashMap<String, Entry>>,
    etag: Arc<AtomicU64>,
    // change broadcast: a simple mpsc that we multiplex per watch
    // we'll keep a global mpsc and let watchers filter
    tx: Arc<tokio::sync::broadcast::Sender<(String, KvEvent)>>,
}

impl Default for InMemoryKvStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryKvStore {
    pub fn new() -> Self {
        let (tx, _rx) = tokio::sync::broadcast::channel(1024);
        let s = Self {
            map: Arc::new(DashMap::new()),
            etag: Arc::new(AtomicU64::new(1)),
            tx: Arc::new(tx),
        };
        // spawn a TTL janitor
        s.spawn_janitor();
        s
    }

    fn now_millis() -> u64 {
        spacetime_std_runtime::now_millis()
    }

    fn spawn_janitor(&self) {
        let map = self.map.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_millis(500));
            loop {
                interval.tick().await;
                let now_ms = Self::now_millis();
                let mut expired: Vec<String> = Vec::new();
                for kv in map.iter() {
                    if let Some(exp) = kv.expire_at {
                        if exp.millis_since_epoch <= now_ms {
                            expired.push(kv.key().clone());
                        }
                    }
                }
                for k in expired {
                    map.remove(&k);
                    let _ = tx.send((k.clone(), KvEvent::Expire { key: k }));
                }
            }
        });
    }

    fn publish(&self, key: String, evt: KvEvent) {
        let _ = self.tx.send((key.clone(), evt.clone()));
    }
}

#[async_trait]
impl KvStore for InMemoryKvStore {
    async fn get(&self, key: &str) -> Result<Option<(Vec<u8>, KvMetadata)>> {
        if let Some(e) = self.map.get(key) {
            // TTL check: if expired, remove and emit Expire, then treat as missing
            if let Some(exp) = e.expire_at {
                let now_ms = Self::now_millis();
                if now_ms >= exp.millis_since_epoch {
                    drop(e);
                    self.map.remove(key);
                    self.publish(
                        key.to_string(),
                        KvEvent::Expire {
                            key: key.to_string(),
                        },
                    );
                    return Ok(None);
                }
            }
            return Ok(Some((e.value.clone(), e.meta.clone())));
        }
        Ok(None)
    }

    async fn put(&self, key: &str, value: &[u8], opts: PutOptions) -> Result<PutResult> {
        let ttl = opts.ttl;
        let mut created = false;
        let etag = self.etag.fetch_add(1, Ordering::SeqCst);
        let expire_at = ttl.map(|d| {
            let now_ms = Self::now_millis();
            let add_ms = d.as_millis() as u64;
            st::Instant::from_millis_since_epoch(now_ms.saturating_add(add_ms))
        });
        let updated_by = "kv".to_string();
        let meta = KvMetadata {
            etag,
            updated_by,
            updated_at: std::time::SystemTime::now(),
            ttl,
        };
        let key_s = key.to_string();
        // two-step logical CAS/update
        if let Some(existing) = self.map.get(&key_s) {
            // exists: enforce CAS if provided
            if let Some(ifm) = opts.if_match {
                if ifm != existing.meta.etag {
                    return Err(anyhow!("etag mismatch"));
                }
            }
            drop(existing);
            self.map.insert(
                key_s.clone(),
                Entry {
                    value: value.to_vec(),
                    meta: meta.clone(),
                    expire_at,
                },
            );
        } else {
            // not exists. Etags start at 1, so etag 0 is never real —
            // `if_match: Some(0)` is the canonical create-if-absent intent
            // (the ceremony runner's cas_put-create). Honor it as a create; a
            // non-zero if_match can't match a missing key, so that stays a
            // mismatch. Keeps InMemory and filesystem backends consistent.
            match opts.if_match {
                None | Some(0) => {}
                Some(_) => return Err(anyhow!("etag mismatch")),
            }
            created = true;
            self.map.insert(
                key_s.clone(),
                Entry {
                    value: value.to_vec(),
                    meta: meta.clone(),
                    expire_at,
                },
            );
        }
        // publish
        let ttl_ms = ttl.map(|d| d.as_millis() as u64);
        self.publish(
            key_s.clone(),
            KvEvent::Put {
                key: key_s.clone(),
                etag,
                ttl_ms,
            },
        );
        Ok(if created {
            PutResult::Created { etag }
        } else {
            PutResult::Updated { etag }
        })
    }

    async fn delete(&self, key: &str, if_match: Option<u64>) -> Result<DeleteResult> {
        if let Some(e) = self.map.get(key) {
            if let Some(m) = if_match {
                if m != e.meta.etag {
                    return Err(anyhow!("etag mismatch"));
                }
            }
        }
        let removed = self.map.remove(key).is_some();
        if removed {
            self.publish(
                key.to_string(),
                KvEvent::Delete {
                    key: key.to_string(),
                },
            );
            Ok(DeleteResult::Deleted)
        } else {
            Ok(DeleteResult::NotFound)
        }
    }

    fn list_prefix(&self, prefix: &str) -> Result<ReceiverStream<(String, Vec<u8>, KvMetadata)>> {
        let (tx, rx) = mpsc::channel(1024);
        let pref = prefix.to_string();
        let map = self.map.clone();
        tokio::spawn(async move {
            for kv in map.iter() {
                if kv.key().starts_with(&pref) {
                    let _ = tx
                        .send((
                            kv.key().clone(),
                            kv.value().value.clone(),
                            kv.value().meta.clone(),
                        ))
                        .await;
                }
            }
        });
        Ok(ReceiverStream::new(rx))
    }

    fn watch_prefix(&self, prefix: &str) -> Result<ReceiverStream<KvEvent>> {
        let pref = prefix.to_string();
        let rx = self.tx.subscribe();
        let (out_tx, out_rx) = mpsc::channel(1024);
        tokio::spawn(async move {
            let mut bs = tokio_stream::wrappers::BroadcastStream::new(rx);
            while let Some(item) = bs.next().await {
                if let Ok((key, evt)) = item {
                    if key.starts_with(&pref) && out_tx.send(evt).await.is_err() {
                        break;
                    }
                }
            }
        });
        Ok(ReceiverStream::new(out_rx))
    }

    fn list_prefix_paged(
        &self,
        prefix: &str,
        page_size: usize,
        cursor: Option<String>,
    ) -> Result<Page<(String, Vec<u8>, KvMetadata)>> {
        // Collect keys with prefix and sort for deterministic order
        let pref = prefix;
        let mut keys: Vec<String> = Vec::new();
        for kv in self.map.iter() {
            if kv.key().starts_with(pref) {
                keys.push(kv.key().clone());
            }
        }
        keys.sort();
        // Determine start position based on cursor (exclusive)
        let start_idx = if let Some(c) = cursor {
            match keys.binary_search(&c) {
                Ok(i) => i + 1,
                Err(i) => i,
            }
        } else {
            0
        };
        let end_idx = std::cmp::min(start_idx + page_size, keys.len());
        let mut items: Vec<(String, Vec<u8>, KvMetadata)> =
            Vec::with_capacity(end_idx.saturating_sub(start_idx));
        for k in &keys[start_idx..end_idx] {
            if let Some(e) = self.map.get(k) {
                items.push((k.clone(), e.value.clone(), e.meta.clone()));
            }
        }
        let next_cursor = if end_idx < keys.len() {
            Some(keys[end_idx - 1].clone())
        } else {
            None
        };
        Ok(Page { items, next_cursor })
    }
}

// Typed helpers
impl InMemoryKvStore {
    pub async fn get_t<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<(T, KvMetadata)>> {
        if let Some((v, m)) = self.get(key).await? {
            let t = serde_json::from_slice::<T>(&v)?;
            Ok(Some((t, m)))
        } else {
            Ok(None)
        }
    }
    pub async fn put_t<T: serde::Serialize>(
        &self,
        key: &str,
        val: &T,
        opts: PutOptions,
    ) -> Result<PutResult> {
        let bytes = serde_json::to_vec(val)?;
        self.put(key, &bytes, opts).await
    }

    /// Watch a prefix and yield typed values when keys under the prefix are updated.
    /// Only Put events are converted; Delete/Expire are skipped in this stream.
    /// For Delete/Expire awareness, also subscribe to `watch_prefix`.
    pub fn watch_prefix_t<T: serde::de::DeserializeOwned + Send + 'static>(
        &self,
        prefix: &str,
    ) -> Result<ReceiverStream<(String, T, KvMetadata)>> {
        let mut evts = self.watch_prefix(prefix)?;
        let pref = prefix.to_string();
        let this = self.clone();
        let (out_tx, out_rx) = mpsc::channel(1024);
        tokio::spawn(async move {
            use futures::StreamExt;
            while let Some(evt) = evts.next().await {
                if let KvEvent::Put { key, .. } = evt {
                    if key.starts_with(&pref) {
                        if let Ok(Some((val, meta))) = this.get_t::<T>(&key).await {
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

    pub fn watch_prefix_t_with_deletes<T: serde::de::DeserializeOwned + Send + 'static>(
        &self,
        prefix: &str,
    ) -> Result<ReceiverStream<PrefixEvent<T>>> {
        let mut evts = self.watch_prefix(prefix)?;
        let pref = prefix.to_string();
        let this = self.clone();
        let (out_tx, out_rx) = mpsc::channel(1024);
        tokio::spawn(async move {
            use futures::StreamExt;
            use std::collections::HashMap;
            let mut cache: HashMap<String, KvMetadata> = HashMap::new();
            while let Some(evt) = evts.next().await {
                match evt {
                    KvEvent::Put { key, .. } => {
                        if key.starts_with(&pref) {
                            if let Ok(Some((val, meta))) = this.get_t::<T>(&key).await {
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
}
