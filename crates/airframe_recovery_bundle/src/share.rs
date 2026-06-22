//! Shamir-on-KEK split and combine.
//!
//! Operates on a 32-byte KEK. Each share is the byte index byte (1..=N)
//! followed by 32 bytes of share material. Combining any K of the N
//! shares reconstructs the original KEK; combining fewer (or wrong-byte)
//! shares yields a value that won't AEAD-verify against the bundle's
//! ciphertext.
//!
//! This crate intentionally does not perform AEAD; the caller (typically
//! Nanokey) wraps the bundle plaintext under the KEK using its preferred
//! cipher.

use sha2::{Digest, Sha256};
use sharks::{Share as SharksShare, Sharks};
use subtle::ConstantTimeEq;
use zeroize::Zeroize;

use crate::bundle::Kek;
use crate::error::ShareError;

/// A Shamir share of the KEK.
///
/// Wraps the underlying library's share representation and includes the
/// `bundle_version` so a receiving call can detect rollback attacks
/// (a share from an older bundle version is invalid against the current
/// bundle even if Shamir math succeeds).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Share {
    /// The bundle version this share was minted for.
    pub bundle_version: u32,
    /// The opaque share bytes.
    pub bytes: Vec<u8>,
}

impl Share {
    /// Construct a share from raw bytes (used by callers parsing from
    /// storage).
    pub fn from_raw(bundle_version: u32, bytes: Vec<u8>) -> Self {
        Self {
            bundle_version,
            bytes,
        }
    }
}

/// Split a 32-byte KEK into `total` Shamir shares with threshold `k`.
///
/// Each share is tagged with the supplied `bundle_version` so it can be
/// rejected later if the bundle has been rotated.
pub fn split_kek(
    kek: &Kek,
    k: u8,
    total: u8,
    bundle_version: u32,
) -> Result<Vec<Share>, ShareError> {
    if k == 0 || total == 0 || k > total {
        return Err(ShareError::InvalidThreshold { k, n: total });
    }
    let sharks = Sharks(k);
    let dealer = sharks.dealer(kek.as_bytes());
    let shares: Vec<Share> = dealer
        .take(total as usize)
        .map(|s| Share {
            bundle_version,
            bytes: Vec::from(&s),
        })
        .collect();
    Ok(shares)
}

/// Combine K Shamir shares to reconstruct the KEK.
///
/// All shares must agree on `bundle_version`; reject otherwise (rollback
/// defense). The threshold `k` is conveyed implicitly — `sharks` was
/// initialized with the same K when shares were generated, and supplying
/// fewer than K shares will fail; supplying mixed-version shares fails
/// loudly.
pub fn combine_shares(k: u8, shares: &[Share]) -> Result<Kek, ShareError> {
    if k == 0 {
        return Err(ShareError::InvalidThreshold { k, n: 0 });
    }
    if (shares.len() as u8) < k {
        return Err(ShareError::InsufficientShares {
            needed: k,
            provided: shares.len(),
        });
    }
    // All shares must agree on bundle_version.
    if let Some(first) = shares.first() {
        for (i, s) in shares.iter().enumerate().skip(1) {
            if s.bundle_version != first.bundle_version {
                return Err(ShareError::MalformedShare { index: i });
            }
        }
    }
    // Parse each share through sharks.
    let mut parsed: Vec<SharksShare> = Vec::with_capacity(shares.len());
    for (i, s) in shares.iter().enumerate() {
        let parsed_share = SharksShare::try_from(s.bytes.as_slice())
            .map_err(|_| ShareError::MalformedShare { index: i })?;
        parsed.push(parsed_share);
    }
    let sharks = Sharks(k);
    let mut recovered =
        sharks
            .recover(parsed.iter())
            .map_err(|_| ShareError::InsufficientShares {
                needed: k,
                provided: shares.len(),
            })?;
    if recovered.len() != 32 {
        let got = recovered.len();
        recovered.zeroize();
        return Err(ShareError::WrongSecretLength { got });
    }
    let mut kek_bytes = [0u8; 32];
    kek_bytes.copy_from_slice(&recovered);
    // Wipe the reconstructed KEK material from the intermediate buffer before drop.
    recovered.zeroize();
    Ok(Kek(kek_bytes))
}

/// A binding commitment to a share's bytes — `SHA-256` over a domain tag, the
/// `bundle_version`, and the share bytes.
///
/// CEF-H3: `sharks`' Lagrange interpolation has no error detection, so a
/// corrupt or maliciously-substituted share silently produces a *wrong* KEK
/// (caught only later by an AEAD failure, with no attribution). Recording a
/// commitment per share at split/enrollment time — in a *trusted, separate*
/// location (the signed bundle metadata or the server-side trustee enrollment
/// record) — lets [`combine_shares_checked`] reject a bad share up front and
/// name the offender.
///
/// The commitment must NOT travel alongside the share: a tamperer would simply
/// recompute it. It only adds integrity when stored where the attacker can't
/// rewrite it.
pub type ShareCommitment = [u8; 32];

const SHARE_COMMITMENT_DOMAIN: &[u8] = b"airframe:recovery:share-commitment:v1";

/// Compute the [`ShareCommitment`] for a share.
pub fn share_commitment(share: &Share) -> ShareCommitment {
    let mut hasher = Sha256::new();
    hasher.update(SHARE_COMMITMENT_DOMAIN);
    hasher.update(share.bundle_version.to_le_bytes());
    hasher.update(&share.bytes);
    hasher.finalize().into()
}

/// Compute commitments for a set of shares, parallel to the input order.
pub fn commit_shares(shares: &[Share]) -> Vec<ShareCommitment> {
    shares.iter().map(share_commitment).collect()
}

/// Like [`combine_shares`], but first verifies each share against the
/// commitment recorded for it at split/enrollment time. `commitments[i]` is the
/// expected commitment for `shares[i]`.
///
/// Rejects a corrupt or tampered share with [`ShareError::CommitmentMismatch`]
/// (carrying the offending index) BEFORE combining — so a bad share is
/// attributed rather than silently corrupting the reconstructed KEK (CEF-H3).
pub fn combine_shares_checked(
    k: u8,
    shares: &[Share],
    commitments: &[ShareCommitment],
) -> Result<Kek, ShareError> {
    if commitments.len() != shares.len() {
        return Err(ShareError::CommitmentCountMismatch {
            commitments: commitments.len(),
            shares: shares.len(),
        });
    }
    for (i, (s, expected)) in shares.iter().zip(commitments.iter()).enumerate() {
        // Constant-time compare; share bytes are secret material.
        if share_commitment(s).ct_eq(expected).unwrap_u8() != 1 {
            return Err(ShareError::CommitmentMismatch { index: i });
        }
    }
    combine_shares(k, shares)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_kek() -> Kek {
        let mut bytes = [0u8; 32];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(7).wrapping_add(11);
        }
        Kek(bytes)
    }

    #[test]
    fn split_then_combine_returns_original() {
        let kek = sample_kek();
        let original = *kek.as_bytes();
        let shares = split_kek(&kek, 3, 5, 1).unwrap();
        assert_eq!(shares.len(), 5);
        let recovered = combine_shares(3, &shares[..3]).unwrap();
        assert_eq!(recovered.as_bytes(), &original);
    }

    #[test]
    fn combine_with_more_than_k_shares_works() {
        let kek = sample_kek();
        let original = *kek.as_bytes();
        let shares = split_kek(&kek, 3, 5, 1).unwrap();
        let recovered = combine_shares(3, &shares[..4]).unwrap();
        assert_eq!(recovered.as_bytes(), &original);
        let recovered = combine_shares(3, &shares).unwrap();
        assert_eq!(recovered.as_bytes(), &original);
    }

    #[test]
    fn combine_with_fewer_than_k_shares_fails() {
        let kek = sample_kek();
        let shares = split_kek(&kek, 3, 5, 1).unwrap();
        let result = combine_shares(3, &shares[..2]);
        assert!(matches!(
            result,
            Err(ShareError::InsufficientShares {
                needed: 3,
                provided: 2
            })
        ));
    }

    #[test]
    fn combine_with_mixed_versions_fails() {
        let kek = sample_kek();
        let mut shares = split_kek(&kek, 2, 3, 1).unwrap();
        shares[1].bundle_version = 2; // tampered
        let result = combine_shares(2, &shares);
        assert!(matches!(
            result,
            Err(ShareError::MalformedShare { index: 1 })
        ));
    }

    #[test]
    fn invalid_threshold_rejected() {
        let kek = sample_kek();
        assert!(matches!(
            split_kek(&kek, 0, 3, 1),
            Err(ShareError::InvalidThreshold { k: 0, n: 3 })
        ));
        assert!(matches!(
            split_kek(&kek, 4, 3, 1),
            Err(ShareError::InvalidThreshold { k: 4, n: 3 })
        ));
    }

    #[test]
    fn each_share_carries_bundle_version() {
        let kek = sample_kek();
        let shares = split_kek(&kek, 2, 3, 42).unwrap();
        for s in &shares {
            assert_eq!(s.bundle_version, 42);
        }
    }

    #[test]
    fn checked_combine_accepts_untampered_shares() {
        let kek = sample_kek();
        let original = *kek.as_bytes();
        let shares = split_kek(&kek, 3, 5, 1).unwrap();
        let commitments = commit_shares(&shares);
        let recovered = combine_shares_checked(3, &shares[..3], &commitments[..3]).unwrap();
        assert_eq!(recovered.as_bytes(), &original);
    }

    #[test]
    fn checked_combine_rejects_tampered_share_with_attribution() {
        let kek = sample_kek();
        let shares = split_kek(&kek, 3, 5, 1).unwrap();
        let commitments = commit_shares(&shares); // committed to the ORIGINAL bytes
        let mut tampered = shares[..3].to_vec();
        // Flip a byte in the share at index 1 (a corrupt/malicious share). Note
        // the commitment is stored separately, so it still binds the original.
        tampered[1].bytes[5] ^= 0xff;
        let result = combine_shares_checked(3, &tampered, &commitments[..3]);
        assert!(
            matches!(result, Err(ShareError::CommitmentMismatch { index: 1 })),
            "expected CommitmentMismatch at index 1, got {result:?}"
        );
    }

    #[test]
    fn checked_combine_rejects_count_mismatch() {
        let kek = sample_kek();
        let shares = split_kek(&kek, 3, 5, 1).unwrap();
        let commitments = commit_shares(&shares);
        let result = combine_shares_checked(3, &shares[..3], &commitments[..2]);
        assert!(matches!(
            result,
            Err(ShareError::CommitmentCountMismatch {
                commitments: 2,
                shares: 3
            })
        ));
    }

    #[test]
    fn commitment_binds_bundle_version() {
        // Same bytes, different bundle_version → different commitment (rollback
        // defense carries into the commitment too).
        let s1 = Share::from_raw(1, vec![1, 2, 3, 4]);
        let s2 = Share::from_raw(2, vec![1, 2, 3, 4]);
        assert_ne!(share_commitment(&s1), share_commitment(&s2));
    }
}
