use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::entry::AuditEntry;
use crate::error::AuditError;

/// Abstract storage backend for the audit chain.
#[async_trait]
pub trait AuditStore: Send + Sync {
    /// Append an entry. The entry's seq must equal current count.
    async fn append(&self, entry: &AuditEntry) -> Result<(), AuditError>;

    /// Read entries in range [from_seq, to_seq] inclusive.
    async fn read_range(&self, from_seq: u64, to_seq: u64) -> Result<Vec<AuditEntry>, AuditError>;

    /// Get a single entry by sequence number.
    async fn get(&self, seq: u64) -> Result<Option<AuditEntry>, AuditError>;

    /// Total number of entries in the log.
    async fn count(&self) -> Result<u64, AuditError>;

    /// Get the last entry (chain tip). Returns None if empty.
    async fn last(&self) -> Result<Option<AuditEntry>, AuditError>;

    /// Update an existing entry (used to add signature to chain tip).
    async fn update(&self, entry: &AuditEntry) -> Result<(), AuditError>;
}

/// In-memory audit store for tests and single-process use.
pub struct InMemoryAuditStore {
    entries: RwLock<Vec<AuditEntry>>,
}

impl InMemoryAuditStore {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
        }
    }
}

impl Default for InMemoryAuditStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AuditStore for InMemoryAuditStore {
    async fn append(&self, entry: &AuditEntry) -> Result<(), AuditError> {
        let mut entries = self.entries.write().await;
        let expected_seq = entries.len() as u64;
        if entry.seq != expected_seq {
            return Err(AuditError::Store(format!(
                "expected seq {}, got {}",
                expected_seq, entry.seq
            )));
        }
        entries.push(entry.clone());
        Ok(())
    }

    async fn read_range(&self, from_seq: u64, to_seq: u64) -> Result<Vec<AuditEntry>, AuditError> {
        let entries = self.entries.read().await;
        let from = from_seq as usize;
        let to = (to_seq as usize).min(entries.len().saturating_sub(1));
        if from >= entries.len() {
            return Ok(Vec::new());
        }
        Ok(entries[from..=to].to_vec())
    }

    async fn get(&self, seq: u64) -> Result<Option<AuditEntry>, AuditError> {
        let entries = self.entries.read().await;
        Ok(entries.get(seq as usize).cloned())
    }

    async fn count(&self) -> Result<u64, AuditError> {
        let entries = self.entries.read().await;
        Ok(entries.len() as u64)
    }

    async fn last(&self) -> Result<Option<AuditEntry>, AuditError> {
        let entries = self.entries.read().await;
        Ok(entries.last().cloned())
    }

    async fn update(&self, entry: &AuditEntry) -> Result<(), AuditError> {
        let mut entries = self.entries.write().await;
        let idx = entry.seq as usize;
        if idx >= entries.len() {
            return Err(AuditError::NotFound(entry.seq));
        }
        entries[idx] = entry.clone();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::AuditEvent;

    fn test_entry(seq: u64) -> AuditEntry {
        AuditEntry {
            seq,
            timestamp: 1000 + seq,
            prev_hash: String::new(),
            entry_hash: format!("hash_{}", seq),
            event: AuditEvent {
                event_type: "test".into(),
                status: "ok".into(),
                actor: "sys".into(),
                target: None,
                details: None,
            },
            signature: None,
        }
    }

    #[tokio::test]
    async fn append_and_get() {
        let store = InMemoryAuditStore::new();
        store.append(&test_entry(0)).await.unwrap();
        store.append(&test_entry(1)).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 2);
        let e = store.get(0).await.unwrap().unwrap();
        assert_eq!(e.seq, 0);
        let e = store.get(1).await.unwrap().unwrap();
        assert_eq!(e.seq, 1);
        assert!(store.get(2).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn append_wrong_seq() {
        let store = InMemoryAuditStore::new();
        let result = store.append(&test_entry(1)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn read_range_and_last() {
        let store = InMemoryAuditStore::new();
        for i in 0..5 {
            store.append(&test_entry(i)).await.unwrap();
        }
        let range = store.read_range(1, 3).await.unwrap();
        assert_eq!(range.len(), 3);
        assert_eq!(range[0].seq, 1);
        assert_eq!(range[2].seq, 3);

        let last = store.last().await.unwrap().unwrap();
        assert_eq!(last.seq, 4);
    }

    #[tokio::test]
    async fn update_entry() {
        let store = InMemoryAuditStore::new();
        store.append(&test_entry(0)).await.unwrap();
        let mut entry = store.get(0).await.unwrap().unwrap();
        entry.signature = Some("sig".into());
        store.update(&entry).await.unwrap();
        let updated = store.get(0).await.unwrap().unwrap();
        assert_eq!(updated.signature, Some("sig".into()));
    }

    #[tokio::test]
    async fn empty_store() {
        let store = InMemoryAuditStore::new();
        assert_eq!(store.count().await.unwrap(), 0);
        assert!(store.last().await.unwrap().is_none());
        assert!(store.read_range(0, 0).await.unwrap().is_empty());
    }
}
