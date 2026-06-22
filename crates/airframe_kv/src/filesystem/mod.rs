mod header;
mod janitor;
mod metadata;
mod ops;

use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::SystemTime;

use anyhow::{bail, Context, Result};
use base64::Engine;

use crate::{KvEvent, KvMetadata};

/// Filesystem-backed KvStore (scaffolding + early helpers)
///
/// This is the initial skeleton per FILESYSTEM_BACKEND_PLAN. It compiles behind the
/// `kv-fs` feature flag and will be filled in incrementally.
pub struct FilesystemKvStore {
    root: PathBuf,
    etag_counter: AtomicU64,
    tx: tokio::sync::broadcast::Sender<(String, KvEvent)>,
}

impl FilesystemKvStore {
    /// Open (or create) a FilesystemKvStore at the given root directory.
    pub async fn open(root: impl AsRef<Path>) -> Result<Arc<Self>> {
        let root = root.as_ref().to_path_buf();
        tokio::fs::create_dir_all(&root).await?;
        // load or initialize kv.meta etag counter
        let meta_path = root.join("kv.meta");
        let etag = if tokio::fs::try_exists(&meta_path).await? {
            let bytes = tokio::fs::read(&meta_path).await?;
            if bytes.len() == 8 {
                u64::from_le_bytes(bytes.try_into().unwrap_or([0u8; 8]))
            } else {
                1
            }
        } else {
            // initialize with 1
            let one: u64 = 1;
            let mut tmp = meta_path.with_extension("tmp");
            tmp.set_extension("meta.tmp");
            tokio::fs::write(&tmp, one.to_le_bytes()).await?;
            // fsync tmp file
            if let Ok(f) = tokio::fs::OpenOptions::new().read(true).open(&tmp).await {
                let _ = f.sync_all().await;
            }
            // rename atomically and fsync parent dir
            tokio::fs::rename(&tmp, &meta_path).await?;
            if let Some(parent) = meta_path.parent() {
                if let Ok(dirf) = tokio::fs::File::open(parent).await {
                    let _ = dirf.sync_all().await;
                }
            }
            one
        };
        let (tx, _rx) = tokio::sync::broadcast::channel(1024);
        let this = Arc::new(Self {
            root: root.clone(),
            etag_counter: AtomicU64::new(etag),
            tx,
        });
        // spawn TTL janitor
        Self::spawn_janitor(this.clone());
        Ok(this)
    }

    pub(crate) async fn fsync_parent(path: &Path) {
        if let Some(parent) = path.parent() {
            if let Ok(dirf) = tokio::fs::File::open(parent).await {
                let _ = dirf.sync_all().await;
            }
        }
    }

    pub(crate) fn publish(&self, key: String, evt: KvEvent) {
        let _ = self.tx.send((key, evt));
    }

    pub(crate) async fn persist_etag(&self, etag: u64) -> Result<()> {
        // persist new etag value to kv.meta atomically with fsyncs
        let meta_path = self.root.join("kv.meta");
        let tmp = meta_path.with_extension("meta.tmp");
        tokio::fs::write(&tmp, etag.to_le_bytes()).await?;
        // fsync tmp file to ensure contents hit disk
        if let Ok(f) = tokio::fs::OpenOptions::new().read(true).open(&tmp).await {
            let _ = f.sync_all().await;
        }
        // rename atomically then fsync parent dir to persist rename
        tokio::fs::rename(&tmp, &meta_path).await?;
        if let Some(parent) = meta_path.parent() {
            if let Ok(dirf) = tokio::fs::File::open(parent).await {
                let _ = dirf.sync_all().await;
            }
        }
        Ok(())
    }

    pub(crate) async fn next_etag(&self) -> Result<u64> {
        let next = self.etag_counter.fetch_add(1, Ordering::SeqCst);
        // store the just-returned value (counter now points to next+1, but we persist current)
        self.persist_etag(next).await?;
        Ok(next)
    }

    // ------- Key/Path codec helpers (initial implementation) -------

    /// Validate key and split into safe path components under root.
    /// Uses directories for all but the final segment; the final segment is
    /// base64-url-no-pad encoded and suffixed with .kv to form the filename.
    pub(crate) fn key_to_path(&self, key: &str) -> Result<PathBuf> {
        validate_key(key)?;
        let mut comps = key.split('/').collect::<Vec<_>>();
        // separate leaf
        let leaf = comps.pop().expect("validated non-empty");
        let mut p = self.root.clone();
        for c in comps {
            p.push(c);
        }
        // encode leaf to filename-safe base64url without padding
        let enc = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(leaf.as_bytes());
        let filename = format!("{}.kv", enc);
        p.push(filename);
        Ok(p)
    }

    /// Inverse of key_to_path for files under our root. Strips root, decodes
    /// the filename (.kv) as base64-url-no-pad for the leaf, rejoins with dirs.
    pub(crate) fn path_to_key(&self, path: &Path) -> Result<String> {
        Self::path_to_key_with_root(&self.root, path)
    }

    pub(crate) fn path_to_key_with_root(root: &Path, path: &Path) -> Result<String> {
        let rel = path
            .strip_prefix(root)
            .with_context(|| format!("path not under root: {}", path.display()))?;
        let mut segs: Vec<String> = Vec::new();
        for c in rel.components() {
            use std::path::Component;
            match c {
                Component::Normal(os) => {
                    let s = os.to_string_lossy();
                    segs.push(s.into_owned());
                }
                _ => bail!("invalid path component"),
            }
        }
        if segs.is_empty() {
            bail!("empty relative path");
        }
        // last segment must be <base64>.kv
        let file = segs.pop().unwrap();
        if !file.ends_with(".kv") {
            bail!("unexpected file extension");
        }
        let b64 = &file[..file.len() - 3];
        let leaf_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(b64.as_bytes())
            .with_context(|| "invalid base64 leaf")?;
        let leaf = String::from_utf8(leaf_bytes).with_context(|| "leaf not utf8")?;
        // remaining segs + decoded leaf
        if !segs.iter().all(|s| !s.is_empty()) {
            bail!("empty segment in path");
        }
        segs.push(leaf);
        Ok(segs.join("/"))
    }

    // Temporary helper to fabricate KvMetadata for scaffolding in case we need it
    #[allow(dead_code)]
    fn meta_stub(&self) -> KvMetadata {
        let etag = self.etag_counter.fetch_add(1, Ordering::SeqCst);
        KvMetadata {
            etag,
            updated_by: "kv-fs".to_string(),
            updated_at: SystemTime::now(),
            ttl: None,
        }
    }
}

/// Validate a KV key for filesystem mapping: non-empty, no leading '/',
/// no '..' segments, no empty segments.
pub(crate) fn validate_key(key: &str) -> Result<()> {
    if key.is_empty() {
        bail!("key empty");
    }
    if key.starts_with('/') {
        bail!("absolute keys not allowed");
    }
    if key.contains("//") {
        bail!("empty segment");
    }
    for seg in key.split('/') {
        if seg == "." || seg == ".." || seg.is_empty() {
            bail!("invalid segment");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use tempfile::tempdir;

    use super::header::Header;
    use crate::{DeleteResult, KvStore, PutOptions, PutResult};
    use std::time::Duration;

    #[tokio::test]
    async fn fs_put_get_delete_roundtrip() -> Result<()> {
        let dir = tempdir().unwrap();
        let fs = FilesystemKvStore::open(dir.path()).await?;
        let key = "ns/demo";
        // create
        let pr = fs
            .put(
                key,
                b"value",
                PutOptions {
                    ttl: None,
                    if_match: None,
                },
            )
            .await?;
        let etag = match pr {
            PutResult::Created { etag } => etag,
            _ => 0,
        };
        assert!(etag > 0);
        // get
        let (val, meta) = fs.get(key).await?.expect("value present");
        assert_eq!(val, b"value");
        assert_eq!(meta.etag, etag);
        // update with CAS
        let pr2 = fs
            .put(
                key,
                b"value2",
                PutOptions {
                    ttl: None,
                    if_match: Some(etag),
                },
            )
            .await?;
        let etag2 = match pr2 {
            PutResult::Updated { etag } => etag,
            _ => 0,
        };
        assert!(etag2 > etag);
        let (val2, meta2) = fs.get(key).await?.expect("value present");
        assert_eq!(val2, b"value2");
        assert_eq!(meta2.etag, etag2);
        // delete with CAS
        let dr = fs.delete(key, Some(etag2)).await?;
        assert!(matches!(dr, DeleteResult::Deleted));
        assert!(fs.get(key).await?.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn key_to_path_and_back_roundtrip() -> Result<()> {
        let dir = tempdir().unwrap();
        let fs = FilesystemKvStore::open(dir.path()).await?;
        let key = "alpha/beta/gamma";
        let p = fs.key_to_path(key)?;
        assert!(p.starts_with(dir.path()));
        assert!(p.extension().unwrap().to_string_lossy() == "kv");
        let round = fs.path_to_key(&p)?;
        assert_eq!(key, round);
        Ok(())
    }

    #[test]
    fn validate_key_rejects_traversal_and_bad_forms() {
        assert!(validate_key("").is_err());
        assert!(validate_key("/abs").is_err());
        assert!(validate_key("one//two").is_err());
        assert!(validate_key("one/../two").is_err());
        assert!(validate_key("./two").is_err());
        assert!(validate_key("ok/key").is_ok());
    }

    #[test]
    fn header_encode_decode_and_crc_detection() -> Result<()> {
        let h = Header {
            ver: Header::VER,
            etag: 42,
            updated_at_millis: 123456789,
            ttl_deadline_millis: -1,
            value_len: 5,
        };
        let value = b"hello".to_vec();
        let bytes = h.encode_with_value(&value);
        let (h2, v2) = Header::decode_and_validate(&bytes)?;
        assert_eq!(h2.etag, 42);
        assert_eq!(v2, value);
        // Corrupt one byte and expect CRC mismatch
        let mut corrupt = bytes.clone();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 0xFF;
        assert!(Header::decode_and_validate(&corrupt).is_err());
        Ok(())
    }

    #[tokio::test]
    async fn list_prefix_and_pagination() -> Result<()> {
        let dir = tempdir().unwrap();
        let fs = FilesystemKvStore::open(dir.path()).await?;
        fs.put(
            "p/a/1",
            b"v1",
            PutOptions {
                ttl: None,
                if_match: None,
            },
        )
        .await?;
        fs.put(
            "p/a/2",
            b"v2",
            PutOptions {
                ttl: None,
                if_match: None,
            },
        )
        .await?;
        fs.put(
            "p/b/3",
            b"v3",
            PutOptions {
                ttl: None,
                if_match: None,
            },
        )
        .await?;
        // Stream list on prefix p/a/
        let mut rx = fs.list_prefix("p/a/")?;
        let mut seen: Vec<String> = Vec::new();
        while let Some((k, _v, _m)) = rx.next().await {
            seen.push(k);
        }
        seen.sort();
        assert_eq!(seen, vec!["p/a/1", "p/a/2"]);
        // Paged list on prefix p/
        let page1 = fs.list_prefix_paged("p/", 2, None)?;
        assert_eq!(page1.items.len(), 2);
        let cursor = page1.next_cursor.clone();
        let page2 = fs.list_prefix_paged("p/", 2, cursor)?;
        // total 3 items across two pages
        assert!(page1.items.len() + page2.items.len() >= 3);
        Ok(())
    }

    #[tokio::test]
    async fn ttl_janitor_expires_and_emits_event() -> Result<()> {
        use tokio::time::{sleep, Duration as TDuration};
        let dir = tempdir().unwrap();
        let fs = FilesystemKvStore::open(dir.path()).await?;
        let mut ev = fs.watch_prefix("x/")?;
        // Put with short TTL
        let _ = fs
            .put(
                "x/ttl",
                b"val",
                PutOptions {
                    ttl: Some(Duration::from_millis(400)),
                    if_match: None,
                },
            )
            .await?;
        // Expect a Put event first (best effort)
        let mut saw_put = false;
        let mut saw_expire = false;
        let start = std::time::Instant::now();
        while start.elapsed() < TDuration::from_millis(2500) {
            if let Some(evt) = ev.next().await {
                match evt {
                    crate::KvEvent::Put { key, .. } if key == "x/ttl" => {
                        saw_put = true;
                    }
                    crate::KvEvent::Expire { key } if key == "x/ttl" => {
                        saw_expire = true;
                        break;
                    }
                    _ => {}
                }
            } else {
                break;
            }
        }
        // Sleep a bit more to let janitor run at least once
        sleep(TDuration::from_millis(1200)).await;
        // Value should be gone
        let got = fs.get("x/ttl").await?;
        assert!(got.is_none());
        assert!(saw_expire, "expected to see Expire event");
        // put might race; don't assert it strictly
        let _ = saw_put;
        Ok(())
    }
}
