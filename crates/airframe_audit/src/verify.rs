use crate::crypto::AuditCrypto;
use crate::entry::AuditEntry;
use crate::error::AuditError;

/// Result of verifying an audit chain (or a range within it).
#[derive(Debug)]
pub struct VerifyResult {
    /// Number of entries that were checked.
    pub entries_checked: u64,
    /// Number of signatures that were verified.
    pub signatures_verified: u64,
    /// True if the chain (or range) is valid.
    pub valid: bool,
    /// First broken link, if any.
    pub broken_at: Option<u64>,
}

/// Verify a slice of entries for hash-chain continuity and signature validity.
///
/// `expected_start_seq` is the sequence number expected for the first entry.
/// If the first entry in the slice is not the genesis (seq 0), `expected_prev_hash`
/// should be the `entry_hash` of the entry immediately preceding this slice.
///
/// When `require_signatures` is `true` (the chain policy signs every entry), an
/// entry whose `signature` is missing is treated as tampering (signature
/// stripping) and fails verification, rather than being silently accepted.
pub async fn verify_entries(
    entries: &[AuditEntry],
    crypto: &dyn AuditCrypto,
    expected_start_seq: u64,
    expected_prev_hash: Option<&str>,
    require_signatures: bool,
) -> Result<VerifyResult, AuditError> {
    if entries.is_empty() {
        return Ok(VerifyResult {
            entries_checked: 0,
            signatures_verified: 0,
            valid: true,
            broken_at: None,
        });
    }

    let mut signatures_verified = 0u64;

    for (i, entry) in entries.iter().enumerate() {
        let expected_seq = expected_start_seq + i as u64;

        // Check sequence number
        if entry.seq != expected_seq {
            return Ok(VerifyResult {
                entries_checked: i as u64,
                signatures_verified,
                valid: false,
                broken_at: Some(expected_seq),
            });
        }

        // Verify prev_hash link
        let expected_prev = if i == 0 {
            match expected_prev_hash {
                Some(h) => h.to_string(),
                None => {
                    // Genesis entry should have empty prev_hash
                    if entry.seq == 0 {
                        String::new()
                    } else {
                        return Ok(VerifyResult {
                            entries_checked: i as u64,
                            signatures_verified,
                            valid: false,
                            broken_at: Some(entry.seq),
                        });
                    }
                }
            }
        } else {
            entries[i - 1].entry_hash.clone()
        };

        if entry.prev_hash != expected_prev {
            return Ok(VerifyResult {
                entries_checked: i as u64,
                signatures_verified,
                valid: false,
                broken_at: Some(entry.seq),
            });
        }

        // Re-compute canonical hash and compare
        let canonical =
            AuditEntry::canonical_bytes(entry.seq, entry.timestamp, &entry.prev_hash, &entry.event);
        let computed_hash = crypto.digest(&canonical).await?;

        if computed_hash != entry.entry_hash {
            return Ok(VerifyResult {
                entries_checked: i as u64,
                signatures_verified,
                valid: false,
                broken_at: Some(entry.seq),
            });
        }

        // Verify signature. When the chain policy signs every entry
        // (`require_signatures`), a missing signature is tampering (signature
        // stripping) and fails verification rather than being silently skipped.
        match entry.signature {
            Some(ref sig) => {
                let valid = crypto.verify(entry.entry_hash.as_bytes(), sig).await?;
                if !valid {
                    return Ok(VerifyResult {
                        entries_checked: (i + 1) as u64,
                        signatures_verified,
                        valid: false,
                        broken_at: Some(entry.seq),
                    });
                }
                signatures_verified += 1;
            }
            None => {
                if require_signatures {
                    return Ok(VerifyResult {
                        entries_checked: (i + 1) as u64,
                        signatures_verified,
                        valid: false,
                        broken_at: Some(entry.seq),
                    });
                }
            }
        }
    }

    Ok(VerifyResult {
        entries_checked: entries.len() as u64,
        signatures_verified,
        valid: true,
        broken_at: None,
    })
}
