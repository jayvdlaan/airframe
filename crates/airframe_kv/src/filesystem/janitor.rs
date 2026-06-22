use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::header::Header;
use super::metadata::is_expired;
use super::FilesystemKvStore;
use crate::KvEvent;

/// Check whether the given path is a `.kv` file eligible for janitor inspection.
fn is_kv_file(path: &Path) -> bool {
    path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("kv")
}

/// Inspect a single `.kv` file and, if its TTL has expired, delete it and
/// return the decoded key. Returns `None` when the file is not expired or
/// could not be read/parsed.
async fn reap_if_expired(store: &FilesystemKvStore, path: &Path) -> Option<String> {
    let bytes = tokio::fs::read(path).await.ok()?;
    let (hdr, _val) = Header::decode_and_validate(&bytes).ok()?;
    if !is_expired(&hdr) {
        return None;
    }
    let _ = tokio::fs::remove_file(path).await;
    store.path_to_key(path).ok()
}

impl FilesystemKvStore {
    pub(super) fn spawn_janitor(this: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(1000));
            loop {
                interval.tick().await;
                let expired_keys = sweep_expired(&this).await;
                for k in expired_keys {
                    this.publish(k.clone(), KvEvent::Expire { key: k });
                }
            }
        });
    }
}

/// Walk the store root and reap every expired `.kv` entry, returning the
/// list of deleted keys.
async fn sweep_expired(store: &FilesystemKvStore) -> Vec<String> {
    let mut expired_keys = Vec::new();
    let entries = walkdir::WalkDir::new(&store.root)
        .into_iter()
        .filter_map(|e| e.ok());

    for entry in entries {
        let path = entry.path();
        if !is_kv_file(path) {
            continue;
        }
        if let Some(key) = reap_if_expired(store, path).await {
            expired_keys.push(key);
        }
    }
    expired_keys
}
