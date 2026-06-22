use std::time::{Duration, SystemTime};

use spacetime_core as st;

use super::header::Header;
use crate::KvMetadata;

/// Check whether the given header's TTL deadline has passed.
/// Returns `true` if the entry is expired (i.e. has a non-negative deadline
/// that is at or before the current time).
pub(crate) fn is_expired(header: &Header) -> bool {
    if header.ttl_deadline_millis >= 0 {
        let now = st::Instant::from_millis_since_epoch(Header::now_millis() as u64);
        let deadline = st::Instant::from_millis_since_epoch(header.ttl_deadline_millis as u64);
        now >= deadline
    } else {
        false
    }
}

/// Construct a `KvMetadata` from a decoded `Header`, computing the remaining
/// TTL (if any) from the current wall-clock time.  `payload_len` is provided
/// for forward-compatibility but is not stored in the metadata today.
pub(crate) fn header_to_metadata(header: &Header, _payload_len: usize) -> KvMetadata {
    let now_ms = Header::now_millis();
    let ttl = if header.ttl_deadline_millis >= 0 {
        let rem = (header.ttl_deadline_millis - now_ms).max(0) as u64;
        Some(Duration::from_millis(rem))
    } else {
        None
    };
    KvMetadata {
        etag: header.etag,
        updated_by: "kv-fs".to_string(),
        updated_at: SystemTime::UNIX_EPOCH
            + std::time::Duration::from_millis(header.updated_at_millis as u64),
        ttl,
    }
}
