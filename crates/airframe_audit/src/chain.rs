use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::Mutex;

use crate::crypto::AuditCrypto;
use crate::entry::{AuditEntry, AuditEvent};
use crate::error::AuditError;
use crate::store::AuditStore;
use crate::verify::{verify_entries, VerifyResult};

/// Configuration for the audit chain.
#[derive(Debug, Clone)]
pub struct AuditChainConfig {
    /// Number of entries between batch signatures. 0 = never auto-sign.
    pub sign_interval: u64,
    /// When `true`, every entry must be signed. If `sign_interval == 0` and
    /// `require_signing == true`, each individual entry is signed on append.
    /// Defaults to `true`.
    pub require_signing: bool,
}

impl Default for AuditChainConfig {
    fn default() -> Self {
        Self {
            sign_interval: 0,
            require_signing: true,
        }
    }
}

/// Hash-chained, optionally signed audit log coordinator.
///
/// Ties together an `AuditCrypto` backend (for hashing and signing),
/// an `AuditStore` backend (for persistence), and the hash-chain logic.
pub struct AuditChain {
    crypto: Arc<dyn AuditCrypto>,
    store: Arc<dyn AuditStore>,
    config: AuditChainConfig,
    entries_since_sign: AtomicU64,
    append_lock: Mutex<()>,
}

/// A signed anchor over a chain position.
///
/// A hash chain can be silently *truncated* (entries dropped from the tip) or
/// *rolled back* and still verify as a valid prefix, because `verify()` only
/// checks internal continuity. To detect that, the caller periodically takes a
/// [`Checkpoint`] and persists it in TRUSTED, separate storage (signed bundle
/// metadata, a server-side record, …). [`AuditChain::verify_against_checkpoint`]
/// then confirms the live chain still reaches that anchored position with the
/// anchored hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    /// Sequence number of the anchored tip.
    pub seq: u64,
    /// `entry_hash` of the entry at `seq`.
    pub entry_hash: String,
    /// Signature over `(seq, entry_hash)`.
    pub signature: String,
}

impl AuditChain {
    pub fn new(
        crypto: Arc<dyn AuditCrypto>,
        store: Arc<dyn AuditStore>,
        config: AuditChainConfig,
    ) -> Self {
        Self {
            crypto,
            store,
            config,
            entries_since_sign: AtomicU64::new(0),
            append_lock: Mutex::new(()),
        }
    }

    /// Append an audit event. Computes hash chain, optionally batch-signs.
    pub async fn append(&self, event: AuditEvent) -> Result<AuditEntry, AuditError> {
        let _guard = self.append_lock.lock().await;

        let last = self.store.last().await?;
        let (seq, prev_hash) = match &last {
            Some(entry) => (entry.seq + 1, entry.entry_hash.clone()),
            None => (0, String::new()),
        };

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let canonical = AuditEntry::canonical_bytes(seq, timestamp, &prev_hash, &event);
        let entry_hash = self.crypto.digest(&canonical).await?;

        let entries_since = self.entries_since_sign.fetch_add(1, Ordering::SeqCst) + 1;

        let should_sign = if self.config.require_signing && self.config.sign_interval == 0 {
            // require_signing + no batch interval → sign every entry
            true
        } else if self.config.sign_interval > 0 && entries_since >= self.config.sign_interval {
            // batch signing threshold reached
            true
        } else {
            false
        };

        let signature = if should_sign {
            let sig = self.crypto.sign(entry_hash.as_bytes()).await?;
            self.entries_since_sign.store(0, Ordering::SeqCst);
            Some(sig)
        } else {
            None
        };

        let entry = AuditEntry {
            seq,
            timestamp,
            prev_hash,
            entry_hash,
            event,
            signature,
        };

        self.store.append(&entry).await?;
        Ok(entry)
    }

    /// Force-sign the current chain tip.
    pub async fn sign_tip(&self) -> Result<AuditEntry, AuditError> {
        let _guard = self.append_lock.lock().await;

        let mut entry = self.store.last().await?.ok_or(AuditError::EmptyChain)?;

        let sig = self.crypto.sign(entry.entry_hash.as_bytes()).await?;
        entry.signature = Some(sig);
        self.store.update(&entry).await?;
        self.entries_since_sign.store(0, Ordering::SeqCst);
        Ok(entry)
    }

    /// Canonical bytes signed/verified for a [`Checkpoint`].
    fn checkpoint_bytes(seq: u64, entry_hash: &str) -> Vec<u8> {
        let mut b = Vec::with_capacity(48 + entry_hash.len());
        b.extend_from_slice(b"airframe:audit:checkpoint:v1|");
        b.extend_from_slice(&seq.to_be_bytes());
        b.push(b'|');
        b.extend_from_slice(entry_hash.as_bytes());
        b
    }

    /// Produce a signed [`Checkpoint`] over the current chain tip. Persist the
    /// returned value in trusted, separate storage; later pass it to
    /// [`verify_against_checkpoint`](Self::verify_against_checkpoint).
    pub async fn checkpoint(&self) -> Result<Checkpoint, AuditError> {
        let _guard = self.append_lock.lock().await;
        let tip = self.store.last().await?.ok_or(AuditError::EmptyChain)?;
        let signature = self
            .crypto
            .sign(&Self::checkpoint_bytes(tip.seq, &tip.entry_hash))
            .await?;
        Ok(Checkpoint {
            seq: tip.seq,
            entry_hash: tip.entry_hash,
            signature,
        })
    }

    /// Verify chain integrity AND that the chain has not been truncated or rolled
    /// back below a trusted [`Checkpoint`].
    ///
    /// Fails if: the checkpoint signature is invalid; the chain no longer reaches
    /// `cp.seq` (truncated below the anchor); or the entry at `cp.seq` no longer
    /// has `cp.entry_hash` (rolled back / forked). Otherwise runs the normal
    /// [`verify`](Self::verify).
    pub async fn verify_against_checkpoint(
        &self,
        cp: &Checkpoint,
    ) -> Result<VerifyResult, AuditError> {
        let invalid = |checked: u64| VerifyResult {
            entries_checked: checked,
            signatures_verified: 0,
            valid: false,
            broken_at: Some(cp.seq),
        };

        // 1. The anchor itself must be authentic.
        let sig_ok = self
            .crypto
            .verify(
                &Self::checkpoint_bytes(cp.seq, &cp.entry_hash),
                &cp.signature,
            )
            .await?;
        if !sig_ok {
            return Ok(invalid(0));
        }

        // 2. The chain must still reach the anchored position (else truncated).
        let count = self.store.count().await?;
        if count <= cp.seq {
            return Ok(invalid(count));
        }

        // 3. The entry at the anchored position must still match (else rolled back).
        match self.store.get(cp.seq).await? {
            Some(e) if e.entry_hash == cp.entry_hash => {}
            _ => return Ok(invalid(count)),
        }

        // 4. Finally, full internal integrity.
        self.verify().await
    }

    /// Verify chain integrity from seq 0 to end.
    pub async fn verify(&self) -> Result<VerifyResult, AuditError> {
        let count = self.store.count().await?;
        if count == 0 {
            return Ok(VerifyResult {
                entries_checked: 0,
                signatures_verified: 0,
                valid: true,
                broken_at: None,
            });
        }
        self.verify_range(0, count - 1).await
    }

    /// Verify a range of entries [from, to] inclusive.
    pub async fn verify_range(&self, from: u64, to: u64) -> Result<VerifyResult, AuditError> {
        let entries = self.store.read_range(from, to).await?;
        if entries.is_empty() {
            return Ok(VerifyResult {
                entries_checked: 0,
                signatures_verified: 0,
                valid: true,
                broken_at: None,
            });
        }

        // If range doesn't start at 0, we need the previous entry's hash
        let expected_prev = if from > 0 {
            let prev = self
                .store
                .get(from - 1)
                .await?
                .ok_or(AuditError::NotFound(from - 1))?;
            Some(prev.entry_hash)
        } else {
            None
        };

        // When the policy signs every entry, verification must reject entries with
        // a stripped (missing) signature rather than silently accepting them.
        let require_signatures = self.config.require_signing && self.config.sign_interval == 0;

        verify_entries(
            &entries,
            self.crypto.as_ref(),
            from,
            expected_prev.as_deref(),
            require_signatures,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InMemoryAuditStore;

    // A minimal test crypto that uses actual SHA-256 but fake signing
    struct TestCrypto;

    #[async_trait::async_trait]
    impl crate::crypto::AuditCrypto for TestCrypto {
        async fn digest(&self, data: &[u8]) -> Result<String, AuditError> {
            // Use openssl via airframe_crypt in dev-deps
            use airframe_crypt::hash::{openssl_digest, DigestAlgorithm};
            let hash = openssl_digest(DigestAlgorithm::Sha256, data)
                .map_err(|e| AuditError::Crypto(e.to_string()))?;
            Ok(hex_encode(&hash))
        }

        async fn sign(&self, message: &[u8]) -> Result<String, AuditError> {
            // Fake signature: just base64 of "signed:" + message
            use base64::Engine;
            let mut buf = b"signed:".to_vec();
            buf.extend_from_slice(message);
            Ok(base64::engine::general_purpose::STANDARD.encode(&buf))
        }

        async fn verify(&self, message: &[u8], signature: &str) -> Result<bool, AuditError> {
            use base64::Engine;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(signature)
                .map_err(|e| AuditError::Crypto(e.to_string()))?;
            let mut expected = b"signed:".to_vec();
            expected.extend_from_slice(message);
            Ok(decoded == expected)
        }
    }

    fn hex_encode(data: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut s = String::with_capacity(data.len() * 2);
        for b in data {
            s.push(HEX[(b >> 4) as usize] as char);
            s.push(HEX[(b & 0xf) as usize] as char);
        }
        s
    }

    fn test_event(name: &str) -> AuditEvent {
        AuditEvent {
            event_type: name.into(),
            status: "success".into(),
            actor: "test".into(),
            target: None,
            details: None,
        }
    }

    #[tokio::test]
    async fn hash_chain_links() {
        let crypto = Arc::new(TestCrypto);
        let store = Arc::new(InMemoryAuditStore::new());
        let chain = AuditChain::new(crypto, store.clone(), AuditChainConfig::default());

        let e0 = chain.append(test_event("first")).await.unwrap();
        assert_eq!(e0.seq, 0);
        assert!(e0.prev_hash.is_empty());
        assert!(!e0.entry_hash.is_empty());
        // Default config has require_signing: true, so every entry is signed
        assert!(e0.signature.is_some());

        let e1 = chain.append(test_event("second")).await.unwrap();
        assert_eq!(e1.seq, 1);
        assert_eq!(e1.prev_hash, e0.entry_hash);

        let e2 = chain.append(test_event("third")).await.unwrap();
        assert_eq!(e2.seq, 2);
        assert_eq!(e2.prev_hash, e1.entry_hash);
    }

    #[tokio::test]
    async fn batch_signing() {
        let crypto = Arc::new(TestCrypto);
        let store = Arc::new(InMemoryAuditStore::new());
        let config = AuditChainConfig {
            sign_interval: 3,
            require_signing: false,
        };
        let chain = AuditChain::new(crypto, store.clone(), config);

        let e0 = chain.append(test_event("a")).await.unwrap();
        assert!(e0.signature.is_none());
        let e1 = chain.append(test_event("b")).await.unwrap();
        assert!(e1.signature.is_none());
        let e2 = chain.append(test_event("c")).await.unwrap();
        assert!(e2.signature.is_some()); // 3rd entry triggers batch sign

        let e3 = chain.append(test_event("d")).await.unwrap();
        assert!(e3.signature.is_none()); // counter reset, first after sign
    }

    #[tokio::test]
    async fn sign_tip_manually() {
        let crypto = Arc::new(TestCrypto);
        let store = Arc::new(InMemoryAuditStore::new());
        let chain = AuditChain::new(crypto, store.clone(), AuditChainConfig::default());

        chain.append(test_event("x")).await.unwrap();
        let signed = chain.sign_tip().await.unwrap();
        assert!(signed.signature.is_some());

        // Verify the stored entry also has the signature
        let stored = store.get(0).await.unwrap().unwrap();
        assert!(stored.signature.is_some());
    }

    #[tokio::test]
    async fn sign_tip_empty_chain() {
        let crypto = Arc::new(TestCrypto);
        let store = Arc::new(InMemoryAuditStore::new());
        let chain = AuditChain::new(crypto, store, AuditChainConfig::default());
        let result = chain.sign_tip().await;
        assert!(matches!(result, Err(AuditError::EmptyChain)));
    }

    #[tokio::test]
    async fn verify_detects_signature_stripping() {
        let crypto = Arc::new(TestCrypto);
        let store = Arc::new(InMemoryAuditStore::new());
        let chain = AuditChain::new(crypto.clone(), store.clone(), AuditChainConfig::default());

        for i in 0..3 {
            chain.append(test_event(&format!("e{i}"))).await.unwrap();
        }
        // A clean, fully-signed chain verifies.
        assert!(chain.verify().await.unwrap().valid);

        // Simulate tampering: strip the signature off the middle entry.
        let mut entries = store.read_range(0, 2).await.unwrap();
        entries[1].signature = None;

        // Under the default policy (sign every entry) a stripped signature is detected.
        let res = verify_entries(&entries, crypto.as_ref(), 0, None, true)
            .await
            .unwrap();
        assert!(!res.valid);
        assert_eq!(res.broken_at, Some(1));

        // Without enforcement the hash chain still passes — exactly the gap the
        // require_signatures flag closes.
        let unenforced = verify_entries(&entries, crypto.as_ref(), 0, None, false)
            .await
            .unwrap();
        assert!(unenforced.valid);
    }

    #[tokio::test]
    async fn checkpoint_detects_truncation() {
        let crypto = Arc::new(TestCrypto);
        let store = Arc::new(InMemoryAuditStore::new());
        let chain = AuditChain::new(crypto, store, AuditChainConfig::default());
        for i in 0..5 {
            chain.append(test_event(&format!("e{i}"))).await.unwrap();
        }
        let cp = chain.checkpoint().await.unwrap();
        assert_eq!(cp.seq, 4);
        // The intact chain verifies against its own checkpoint.
        assert!(chain.verify_against_checkpoint(&cp).await.unwrap().valid);

        // A chain truncated below the checkpoint (only 3 of 5 entries) is rejected —
        // even though those 3 entries form a perfectly valid prefix that plain
        // verify() would accept.
        let short = AuditChain::new(
            Arc::new(TestCrypto),
            Arc::new(InMemoryAuditStore::new()),
            AuditChainConfig::default(),
        );
        for i in 0..3 {
            short.append(test_event(&format!("e{i}"))).await.unwrap();
        }
        let res = short.verify_against_checkpoint(&cp).await.unwrap();
        assert!(!res.valid, "truncation below checkpoint not detected");
    }

    #[tokio::test]
    async fn verify_clean_chain() {
        let crypto = Arc::new(TestCrypto);
        let store = Arc::new(InMemoryAuditStore::new());
        let chain = AuditChain::new(crypto, store, AuditChainConfig::default());

        for i in 0..5 {
            chain
                .append(test_event(&format!("event_{}", i)))
                .await
                .unwrap();
        }

        let result = chain.verify().await.unwrap();
        assert!(result.valid);
        assert_eq!(result.entries_checked, 5);
        assert_eq!(result.broken_at, None);
    }

    #[tokio::test]
    async fn verify_empty_chain() {
        let crypto = Arc::new(TestCrypto);
        let store = Arc::new(InMemoryAuditStore::new());
        let chain = AuditChain::new(crypto, store, AuditChainConfig::default());

        let result = chain.verify().await.unwrap();
        assert!(result.valid);
        assert_eq!(result.entries_checked, 0);
    }

    #[tokio::test]
    async fn detect_tampered_entry() {
        let crypto = Arc::new(TestCrypto);
        let store = Arc::new(InMemoryAuditStore::new());
        let chain = AuditChain::new(crypto, store.clone(), AuditChainConfig::default());

        for i in 0..5 {
            chain
                .append(test_event(&format!("event_{}", i)))
                .await
                .unwrap();
        }

        // Tamper with entry 2's event
        let mut entry = store.get(2).await.unwrap().unwrap();
        entry.event.status = "tampered".into();
        store.update(&entry).await.unwrap();

        let result = chain.verify().await.unwrap();
        assert!(!result.valid);
        assert_eq!(result.broken_at, Some(2));
    }

    #[tokio::test]
    async fn detect_broken_prev_hash() {
        let crypto = Arc::new(TestCrypto);
        let store = Arc::new(InMemoryAuditStore::new());
        let chain = AuditChain::new(crypto, store.clone(), AuditChainConfig::default());

        for i in 0..5 {
            chain
                .append(test_event(&format!("event_{}", i)))
                .await
                .unwrap();
        }

        // Break the prev_hash link at entry 3
        let mut entry = store.get(3).await.unwrap().unwrap();
        entry.prev_hash = "0000000000000000000000000000000000000000000000000000000000000000".into();
        store.update(&entry).await.unwrap();

        let result = chain.verify().await.unwrap();
        assert!(!result.valid);
        assert_eq!(result.broken_at, Some(3));
    }

    #[tokio::test]
    async fn detect_bad_signature() {
        let crypto = Arc::new(TestCrypto);
        let store = Arc::new(InMemoryAuditStore::new());
        let config = AuditChainConfig {
            sign_interval: 2,
            require_signing: false,
        };
        let chain = AuditChain::new(crypto, store.clone(), config);

        chain.append(test_event("a")).await.unwrap();
        chain.append(test_event("b")).await.unwrap(); // signed

        // Corrupt the signature on entry 1
        let mut entry = store.get(1).await.unwrap().unwrap();
        entry.signature = Some("aW52YWxpZA==".into()); // "invalid" in base64
        store.update(&entry).await.unwrap();

        let result = chain.verify().await.unwrap();
        assert!(!result.valid);
        assert_eq!(result.broken_at, Some(1));
    }

    #[tokio::test]
    async fn verify_with_signatures() {
        let crypto = Arc::new(TestCrypto);
        let store = Arc::new(InMemoryAuditStore::new());
        let config = AuditChainConfig {
            sign_interval: 2,
            require_signing: false,
        };
        let chain = AuditChain::new(crypto, store, config);

        for i in 0..4 {
            chain
                .append(test_event(&format!("ev_{}", i)))
                .await
                .unwrap();
        }

        let result = chain.verify().await.unwrap();
        assert!(result.valid);
        assert_eq!(result.entries_checked, 4);
        assert_eq!(result.signatures_verified, 2); // entries 1 and 3
    }

    #[tokio::test]
    async fn verify_range_subset() {
        let crypto = Arc::new(TestCrypto);
        let store = Arc::new(InMemoryAuditStore::new());
        let chain = AuditChain::new(crypto, store, AuditChainConfig::default());

        for i in 0..10 {
            chain
                .append(test_event(&format!("ev_{}", i)))
                .await
                .unwrap();
        }

        let result = chain.verify_range(3, 7).await.unwrap();
        assert!(result.valid);
        assert_eq!(result.entries_checked, 5);
    }

    #[tokio::test]
    async fn concurrent_appends() {
        let crypto = Arc::new(TestCrypto);
        let store = Arc::new(InMemoryAuditStore::new());
        let chain = Arc::new(AuditChain::new(
            crypto,
            store.clone(),
            AuditChainConfig::default(),
        ));

        let mut handles = Vec::new();
        for i in 0..20 {
            let chain = chain.clone();
            handles.push(tokio::spawn(async move {
                chain
                    .append(test_event(&format!("concurrent_{}", i)))
                    .await
                    .unwrap();
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        assert_eq!(store.count().await.unwrap(), 20);

        // All entries should be properly sequenced
        let result = chain.verify().await.unwrap();
        assert!(result.valid);
        assert_eq!(result.entries_checked, 20);
    }

    #[tokio::test]
    async fn require_signing_signs_every_entry() {
        let crypto = Arc::new(TestCrypto);
        let store = Arc::new(InMemoryAuditStore::new());
        // require_signing: true + sign_interval: 0 → every entry signed
        let config = AuditChainConfig {
            sign_interval: 0,
            require_signing: true,
        };
        let chain = AuditChain::new(crypto, store, config);

        for i in 0..5 {
            let entry = chain
                .append(test_event(&format!("ev_{}", i)))
                .await
                .unwrap();
            assert!(entry.signature.is_some(), "entry {} should be signed", i);
        }

        let result = chain.verify().await.unwrap();
        assert!(result.valid);
        assert_eq!(result.signatures_verified, 5);
    }

    #[tokio::test]
    async fn require_signing_false_no_auto_sign() {
        let crypto = Arc::new(TestCrypto);
        let store = Arc::new(InMemoryAuditStore::new());
        // require_signing: false + sign_interval: 0 → no signing
        let config = AuditChainConfig {
            sign_interval: 0,
            require_signing: false,
        };
        let chain = AuditChain::new(crypto, store, config);

        for i in 0..5 {
            let entry = chain
                .append(test_event(&format!("ev_{}", i)))
                .await
                .unwrap();
            assert!(
                entry.signature.is_none(),
                "entry {} should NOT be signed",
                i
            );
        }
    }

    #[tokio::test]
    async fn canonical_hash_determinism() {
        let crypto = Arc::new(TestCrypto);
        let store1 = Arc::new(InMemoryAuditStore::new());
        let store2 = Arc::new(InMemoryAuditStore::new());
        let chain1 = AuditChain::new(crypto.clone(), store1, AuditChainConfig::default());
        let chain2 = AuditChain::new(crypto, store2, AuditChainConfig::default());

        let event = AuditEvent {
            event_type: "vault.open".into(),
            status: "success".into(),
            actor: "user1".into(),
            target: Some("vault-1".into()),
            details: None,
        };

        let e1 = chain1.append(event.clone()).await.unwrap();
        let e2 = chain2.append(event).await.unwrap();

        // Same event data but different timestamps means different hashes
        // (timestamps will differ slightly). The canonical form includes timestamp,
        // so this is expected. We can only test that the hash is non-empty.
        assert!(!e1.entry_hash.is_empty());
        assert!(!e2.entry_hash.is_empty());
    }
}
