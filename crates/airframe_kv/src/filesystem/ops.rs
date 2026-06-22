use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::header::Header;
use super::metadata::{header_to_metadata, is_expired};
use super::{validate_key, FilesystemKvStore};
use crate::{DeleteResult, KvEvent, KvStore, Page, PutOptions, PutResult};

#[async_trait]
impl KvStore for FilesystemKvStore {
    async fn get(&self, key: &str) -> Result<Option<(Vec<u8>, crate::KvMetadata)>> {
        validate_key(key)?;
        let path = self.key_to_path(key)?;
        if !tokio::fs::try_exists(&path).await? {
            return Ok(None);
        }
        let bytes = tokio::fs::read(&path).await?;
        let (hdr, value) = Header::decode_and_validate(&bytes)?;
        // TTL check
        if is_expired(&hdr) {
            // expired: best-effort delete
            let _ = tokio::fs::remove_file(&path).await;
            return Ok(None);
        }
        let meta = header_to_metadata(&hdr, value.len());
        Ok(Some((value, meta)))
    }

    async fn put(&self, key: &str, value: &[u8], opts: PutOptions) -> Result<PutResult> {
        validate_key(key)?;
        let path = self.key_to_path(key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let exists = tokio::fs::try_exists(&path).await?;
        let mut created = false;
        if exists {
            let bytes = tokio::fs::read(&path).await?;
            let (hdr, _val) = Header::decode_and_validate(&bytes)?;
            if let Some(ifm) = opts.if_match {
                if ifm != hdr.etag {
                    return Err(anyhow!("etag mismatch"));
                }
            }
        } else {
            // No existing entry. Etag counters start at 1, so etag 0 is never a
            // real stored value — `if_match: Some(0)` is the canonical
            // "expect-absent / create-if-absent" intent (used by the ceremony
            // runner's cas_put-create). Honor it as a create. A non-zero
            // if_match cannot match a missing key, so that stays a mismatch.
            match opts.if_match {
                None | Some(0) => created = true,
                Some(_) => return Err(anyhow!("etag mismatch")),
            }
        }
        let new_etag = self.next_etag().await?;
        let now_ms = Header::now_millis();
        let ttl_deadline = match opts.ttl {
            Some(d) => (now_ms as i128 + d.as_millis() as i128) as i64,
            None => -1,
        };
        let hdr = Header {
            ver: Header::VER,
            etag: new_etag,
            updated_at_millis: now_ms,
            ttl_deadline_millis: ttl_deadline,
            value_len: value.len() as u64,
        };
        let bytes = hdr.encode_with_value(value);
        // write to temp then fsync, rename, fsync parent dir.
        // The temp file is created owner-only (0600) on Unix so KV blobs (which
        // include sealed keystore material) are not readable by other local users;
        // the mode survives the rename. A fresh create_new ensures 0600 actually
        // applies. (Pre-existing files keep their old mode until next write.)
        let tmp = path.with_extension("kv.tmp");
        let _ = tokio::fs::remove_file(&tmp).await;
        let mut f = {
            let mut opts = tokio::fs::OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            opts.mode(0o600); // tokio's OpenOptions exposes mode() inherently on Unix
            opts.open(&tmp).await?
        };
        {
            use tokio::io::AsyncWriteExt;
            f.write_all(&bytes).await?;
        }
        // fsync the tmp file to ensure content is on disk before rename
        let _ = f.sync_all().await;
        drop(f);
        tokio::fs::rename(&tmp, &path).await?;
        // fsync parent directory to persist the rename
        Self::fsync_parent(&path).await;
        // publish event
        let ttl_ms = opts.ttl.map(|d| d.as_millis() as u64);
        self.publish(
            key.to_string(),
            KvEvent::Put {
                key: key.to_string(),
                etag: new_etag,
                ttl_ms,
            },
        );
        Ok(if created {
            PutResult::Created { etag: new_etag }
        } else {
            PutResult::Updated { etag: new_etag }
        })
    }

    async fn delete(&self, key: &str, if_match: Option<u64>) -> Result<DeleteResult> {
        validate_key(key)?;
        let path = self.key_to_path(key)?;
        if !tokio::fs::try_exists(&path).await? {
            return Ok(DeleteResult::NotFound);
        }
        if let Some(m) = if_match {
            let bytes = tokio::fs::read(&path).await?;
            let (hdr, _val) = Header::decode_and_validate(&bytes)?;
            if m != hdr.etag {
                return Err(anyhow!("etag mismatch"));
            }
        }
        tokio::fs::remove_file(&path).await?;
        // fsync parent directory to persist the unlink metadata
        Self::fsync_parent(&path).await;
        // publish
        self.publish(
            key.to_string(),
            KvEvent::Delete {
                key: key.to_string(),
            },
        );
        Ok(DeleteResult::Deleted)
    }

    fn list_prefix(
        &self,
        prefix: &str,
    ) -> Result<ReceiverStream<(String, Vec<u8>, crate::KvMetadata)>> {
        let (tx, rx) = mpsc::channel(1024);
        let pref = prefix.to_string();
        let root = self.root.clone();
        tokio::spawn(async move {
            // Walk directory and stream items that match prefix
            for entry in walkdir::WalkDir::new(&root)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path().to_path_buf();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("kv") {
                    if let Ok(key) = FilesystemKvStore::path_to_key_with_root(&root, &path) {
                        if key.starts_with(&pref) {
                            if let Ok(bytes) = tokio::fs::read(&path).await {
                                if let Ok((hdr, value)) = Header::decode_and_validate(&bytes) {
                                    if is_expired(&hdr) {
                                        continue;
                                    }
                                    let meta = header_to_metadata(&hdr, value.len());
                                    let _ = tx.send((key, value, meta)).await;
                                }
                            }
                        }
                    }
                }
            }
        });
        Ok(ReceiverStream::new(rx))
    }

    fn list_prefix_paged(
        &self,
        prefix: &str,
        page_size: usize,
        cursor: Option<String>,
    ) -> Result<Page<(String, Vec<u8>, crate::KvMetadata)>> {
        let pref = prefix.to_string();
        let mut keys: Vec<String> = Vec::new();
        for entry in walkdir::WalkDir::new(&self.root)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("kv") {
                if let Ok(key) = self.path_to_key(path) {
                    if key.starts_with(&pref) {
                        keys.push(key);
                    }
                }
            }
        }
        keys.sort();
        let start_idx = if let Some(c) = cursor {
            match keys.binary_search(&c) {
                Ok(i) => i + 1,
                Err(i) => i,
            }
        } else {
            0
        };
        let end_idx = std::cmp::min(start_idx + page_size, keys.len());
        let mut items: Vec<(String, Vec<u8>, crate::KvMetadata)> =
            Vec::with_capacity(end_idx.saturating_sub(start_idx));
        for k in &keys[start_idx..end_idx] {
            let path = self.key_to_path(k)?;
            if let Ok(bytes) = std::fs::read(&path) {
                // blocking ok for small local lists
                if let Ok((hdr, value)) = Header::decode_and_validate(&bytes) {
                    if is_expired(&hdr) {
                        continue;
                    }
                    let meta = header_to_metadata(&hdr, value.len());
                    items.push((k.clone(), value, meta));
                }
            }
        }
        let next_cursor = if end_idx < keys.len() {
            Some(keys[end_idx - 1].clone())
        } else {
            None
        };
        Ok(Page { items, next_cursor })
    }

    fn watch_prefix(&self, prefix: &str) -> Result<ReceiverStream<KvEvent>> {
        let pref = prefix.to_string();
        let rx = self.tx.subscribe();
        let (out_tx, out_rx) = mpsc::channel(1024);
        tokio::spawn(async move {
            let mut bs = tokio_stream::wrappers::BroadcastStream::new(rx);
            use futures::StreamExt;
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
}
