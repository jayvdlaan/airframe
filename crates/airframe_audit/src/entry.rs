use serde::{Deserialize, Serialize};

/// A single audit log event (domain-specific payload).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Event type identifier, e.g. "vault.open", "session.create", "admin.seal"
    pub event_type: String,
    /// Operation outcome: "success", "failure", "denied"
    pub status: String,
    /// Actor identifier (user_id, session_id, "system", etc.)
    pub actor: String,
    /// Optional target resource identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Optional structured details (redacted -- no secrets)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// A hash-chained audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Monotonic sequence number (0-indexed)
    pub seq: u64,
    /// Unix timestamp (seconds) when the entry was created
    pub timestamp: u64,
    /// SHA-256 hash of the previous entry (hex). Empty string for seq=0.
    pub prev_hash: String,
    /// SHA-256 hash of this entry's canonical form (hex)
    pub entry_hash: String,
    /// The audit event payload
    pub event: AuditEvent,
    /// Optional Ed25519 signature over `entry_hash` (base64).
    /// Present on batch-signed chain tip entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

impl AuditEntry {
    /// Build the canonical byte representation for hashing:
    /// seq (8-byte BE) || timestamp (8-byte BE) || prev_hash (UTF-8) || JSON(event)
    ///
    /// JSON(event) uses sorted keys for deterministic serialization.
    pub fn canonical_bytes(
        seq: u64,
        timestamp: u64,
        prev_hash: &str,
        event: &AuditEvent,
    ) -> Vec<u8> {
        let event_json = canonical_json(event);
        let mut buf = Vec::with_capacity(8 + 8 + prev_hash.len() + event_json.len());
        buf.extend_from_slice(&seq.to_be_bytes());
        buf.extend_from_slice(&timestamp.to_be_bytes());
        buf.extend_from_slice(prev_hash.as_bytes());
        buf.extend_from_slice(event_json.as_bytes());
        buf
    }
}

/// Produce deterministic JSON with sorted keys for an AuditEvent.
fn canonical_json(event: &AuditEvent) -> String {
    // Serialize to serde_json::Value first, then sort keys recursively
    let value = serde_json::to_value(event).expect("AuditEvent is always serializable");
    let sorted = sort_json_value(&value);
    serde_json::to_string(&sorted).expect("sorted JSON is always serializable")
}

fn sort_json_value(val: &serde_json::Value) -> serde_json::Value {
    match val {
        serde_json::Value::Object(map) => {
            let mut sorted: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for k in keys {
                sorted.insert(k.clone(), sort_json_value(&map[k]));
            }
            serde_json::Value::Object(sorted)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(sort_json_value).collect())
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_json_is_deterministic() {
        let event = AuditEvent {
            event_type: "vault.open".into(),
            status: "success".into(),
            actor: "user1".into(),
            target: Some("vault-abc".into()),
            details: None,
        };
        let json1 = canonical_json(&event);
        let json2 = canonical_json(&event);
        assert_eq!(json1, json2);
        // Keys should be sorted alphabetically
        assert!(json1.find("\"actor\"").unwrap() < json1.find("\"event_type\"").unwrap());
        assert!(json1.find("\"event_type\"").unwrap() < json1.find("\"status\"").unwrap());
    }

    #[test]
    fn canonical_bytes_deterministic() {
        let event = AuditEvent {
            event_type: "test".into(),
            status: "ok".into(),
            actor: "sys".into(),
            target: None,
            details: None,
        };
        let b1 = AuditEntry::canonical_bytes(0, 1000, "", &event);
        let b2 = AuditEntry::canonical_bytes(0, 1000, "", &event);
        assert_eq!(b1, b2);
    }
}
