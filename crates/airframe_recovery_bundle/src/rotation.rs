//! Method-rotation primitives.
//!
//! Builds new [`RecoveryBundle`]s from existing ones for the
//! add-method, remove-method, and rotate-credential ceremonies.
//! Each operation:
//!
//! - Increments `bundle_version` (load-bearing for share-rollback defense)
//! - Mints a fresh `bundle_id` and chains `prior_bundle_id` to the prior bundle
//! - Carries the policy's constraints forward unchanged
//! - Re-Shamirs the (caller-supplied) new KEK to the new (K, N) threshold
//!
//! # AEAD ownership
//!
//! This crate does NOT perform AEAD. Production rotation runs inside
//! Nanokey, which:
//!
//! 1. Combines the K presented shares to recover the OLD KEK
//! 2. AEAD-decrypts the old bundle's ciphertext to recover the
//!    [`BundlePayload`](crate::BundlePayload)
//! 3. Generates a FRESH KEK
//! 4. AEAD-encrypts the payload under the fresh KEK
//! 5. Calls into this module with `(old_bundle, new_kek, new_ciphertext)`
//!    to produce the new envelope and new shares
//! 6. Persists the new envelope via the bundle CRUD endpoint and
//!    returns the new shares to the operator (for the new method holder)
//!
//! The single-key atomic commit happens at step 6 — Nanokey's
//! `PUT /admin/bundles/{id}` is the load-bearing CAS write.

use airframe_id::Threshold;

use crate::bundle::{Kek, RecoveryBundle};
use crate::error::ShareError;
use crate::policy::RecoveryPolicy;
use crate::share::{split_kek, Share};

/// Result returned by [`rotate_add_method`] and [`rotate_remove_method`].
#[derive(Clone, Debug)]
pub struct RotatedBundle {
    /// The new envelope. Caller persists this.
    pub bundle: RecoveryBundle,
    /// The new shares. Caller distributes per the new method-holder set.
    pub shares: Vec<Share>,
}

/// Compute the rotated bundle for **adding a method** to an existing
/// recovery set.
///
/// Threshold semantics: K stays the same; N increments by 1 (the new
/// method-holder gets the new share). The threshold ratio decreases
/// (e.g., 2-of-3 → 2-of-4) — this is deliberate; tightening K
/// alongside N is the [`rotate_change_threshold`] ceremony, separate.
///
/// # Errors
///
/// - [`ShareError::InvalidThreshold`] if `new_n` ≤ the old N (no growth).
/// - Any error from [`split_kek`].
///
/// # Caller responsibilities
///
/// The `new_ciphertext` MUST be the AEAD-encryption of the same
/// [`BundlePayload`](crate::BundlePayload) under `new_kek`. This module cannot enforce that
/// — the AEAD lives in Nanokey. If they don't match, the new bundle
/// will fail to decrypt at recovery time (the AEAD tag check).
pub fn rotate_add_method(
    old_bundle: &RecoveryBundle,
    new_kek: &Kek,
    new_ciphertext: Vec<u8>,
) -> Result<RotatedBundle, ShareError> {
    let old = old_bundle.policy.threshold;
    let new_n = old.n + 1;
    let new_threshold = Threshold::new(old.k, new_n).ok_or(ShareError::InvalidThreshold {
        k: old.k as u8,
        n: new_n as u8,
    })?;
    rotate_with_threshold(old_bundle, new_kek, new_ciphertext, new_threshold)
}

/// Compute the rotated bundle for **rotating a method's credential** —
/// e.g., the user's hardware token was suspected of compromise and
/// they want forward security against any prior copies of the share.
///
/// Treated as remove + add atomically: N stays the same, K stays the
/// same, but every share gets a fresh polynomial. Old shares no
/// longer reconstruct the new KEK; that's the forward-security
/// property.
///
/// # Errors
///
/// Any error from [`split_kek`].
///
/// # Caller responsibilities
///
/// Same as [`rotate_add_method`]: caller has decrypted the old
/// bundle, generated `new_kek`, and AEAD-wrapped the same payload
/// under it — all inside Nanokey.
pub fn rotate_credential(
    old_bundle: &RecoveryBundle,
    new_kek: &Kek,
    new_ciphertext: Vec<u8>,
) -> Result<RotatedBundle, ShareError> {
    // K and N both stay the same; we just bump bundle_version and
    // re-Shamir under a fresh polynomial.
    let preserved = old_bundle.policy.threshold;
    rotate_with_threshold(old_bundle, new_kek, new_ciphertext, preserved)
}

/// Compute the rotated bundle for **changing the threshold K** of an
/// existing recovery set. N stays the same; K moves to `new_k`.
///
/// Authorization required (enforced by the caller): `max(K_old, K_new)`
/// approvers must sign the rotation. The reasoning: lowering K
/// requires K_old approvers (to authorize against the current
/// policy); raising K requires K_new approvers (so the resulting
/// policy is feasible). Either direction, the binding constraint is
/// the larger of the two.
///
/// # Errors
///
/// - [`ShareError::InvalidThreshold`] if `new_k > N`, `new_k == 0`,
///   or new_k > N (covered by `Threshold::new`).
/// - Any error from [`split_kek`].
pub fn rotate_change_threshold(
    old_bundle: &RecoveryBundle,
    new_k: u32,
    new_kek: &Kek,
    new_ciphertext: Vec<u8>,
) -> Result<RotatedBundle, ShareError> {
    let n = old_bundle.policy.threshold.n;
    let new_threshold = Threshold::new(new_k, n).ok_or(ShareError::InvalidThreshold {
        k: new_k as u8,
        n: n as u8,
    })?;
    rotate_with_threshold(old_bundle, new_kek, new_ciphertext, new_threshold)
}

/// Returns `max(K_old, K_new)` — the authorization quorum required
/// for a change-threshold rotation. Exposed so the calling layer
/// can compute the required approver count without reproducing the
/// rule.
pub fn change_threshold_required_quorum(old_k: u32, new_k: u32) -> u32 {
    old_k.max(new_k)
}

/// Compute the rotated bundle for **removing a method** from an
/// existing recovery set (possessed-method removal).
///
/// Threshold semantics: K stays the same; N decrements by 1. Refuses
/// to make K > N (the threshold table forbids it via
/// [`Threshold::new`]).
///
/// # Errors
///
/// - [`ShareError::InvalidThreshold`] if removal would make K > N
///   (i.e., trying to remove from a unanimous set).
/// - Any error from [`split_kek`].
pub fn rotate_remove_method(
    old_bundle: &RecoveryBundle,
    new_kek: &Kek,
    new_ciphertext: Vec<u8>,
) -> Result<RotatedBundle, ShareError> {
    let old = old_bundle.policy.threshold;
    if old.n == 0 {
        return Err(ShareError::InvalidThreshold {
            k: old.k as u8,
            n: 0,
        });
    }
    let new_n = old.n - 1;
    let new_threshold = Threshold::new(old.k, new_n).ok_or(ShareError::InvalidThreshold {
        k: old.k as u8,
        n: new_n as u8,
    })?;
    rotate_with_threshold(old_bundle, new_kek, new_ciphertext, new_threshold)
}

/// Internal helper: build the new envelope + shares given an arbitrary
/// new threshold. Both `rotate_add_method` and `rotate_remove_method`
/// call this; future change-threshold ceremonies will too.
fn rotate_with_threshold(
    old_bundle: &RecoveryBundle,
    new_kek: &Kek,
    new_ciphertext: Vec<u8>,
    new_threshold: Threshold,
) -> Result<RotatedBundle, ShareError> {
    let new_policy = RecoveryPolicy {
        threshold: new_threshold,
        constraints: old_bundle.policy.constraints.clone(),
    };
    let new_version = old_bundle.bundle_version + 1;
    let mut new_bundle = RecoveryBundle::new(
        new_version,
        Some(old_bundle.bundle_id),
        new_policy,
        old_bundle.scheme.clone(),
        new_ciphertext,
    );
    // RecoveryBundle::new sets prior_bundle_id from the second arg
    // — verified above. Patch nothing else; the new bundle is its
    // own entity.
    new_bundle.prior_bundle_id = Some(old_bundle.bundle_id);

    let shares = split_kek(
        new_kek,
        new_threshold.k as u8,
        new_threshold.n as u8,
        new_version,
    )?;
    Ok(RotatedBundle {
        bundle: new_bundle,
        shares,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{MethodRoleKind, RecoveryConstraint, ShareScheme};
    use crate::share::combine_shares;

    fn fixture_kek(seed: u8) -> Kek {
        let mut bytes = [0u8; 32];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = (i as u8)
                .wrapping_mul(seed.wrapping_add(7))
                .wrapping_add(seed);
        }
        Kek(bytes)
    }

    fn build_initial_bundle(k: u32, n: u32, version: u32) -> RecoveryBundle {
        RecoveryBundle::new(
            version,
            None,
            RecoveryPolicy {
                threshold: Threshold::new(k, n).unwrap(),
                constraints: vec![RecoveryConstraint::AtLeastOneOfRole {
                    role: MethodRoleKind::ExternalTrustee,
                }],
            },
            ShareScheme::ShamirAeadV1,
            vec![0xab, 0xcd, 0xef],
        )
    }

    // -- add-method ----------------------------------------------------

    #[test]
    fn add_method_increments_bundle_version() {
        let old = build_initial_bundle(2, 3, 7);
        let kek = fixture_kek(11);
        let rotated = rotate_add_method(&old, &kek, vec![1, 2, 3]).unwrap();
        assert_eq!(rotated.bundle.bundle_version, 8);
    }

    #[test]
    fn add_method_increments_n_by_one() {
        let old = build_initial_bundle(2, 3, 1);
        let kek = fixture_kek(13);
        let rotated = rotate_add_method(&old, &kek, vec![1]).unwrap();
        assert_eq!(rotated.bundle.policy.threshold.n, 4);
    }

    #[test]
    fn add_method_preserves_threshold_k() {
        let old = build_initial_bundle(3, 5, 1);
        let kek = fixture_kek(17);
        let rotated = rotate_add_method(&old, &kek, vec![1]).unwrap();
        assert_eq!(rotated.bundle.policy.threshold.k, 3);
        assert_eq!(rotated.bundle.policy.threshold.n, 6);
    }

    #[test]
    fn add_method_preserves_policy_constraints() {
        let old = build_initial_bundle(2, 3, 1);
        let kek = fixture_kek(19);
        let rotated = rotate_add_method(&old, &kek, vec![1]).unwrap();
        assert_eq!(rotated.bundle.policy.constraints, old.policy.constraints);
    }

    #[test]
    fn add_method_chains_prior_bundle_id() {
        let old = build_initial_bundle(2, 3, 1);
        let kek = fixture_kek(23);
        let rotated = rotate_add_method(&old, &kek, vec![1]).unwrap();
        assert_eq!(rotated.bundle.prior_bundle_id, Some(old.bundle_id));
        assert_ne!(rotated.bundle.bundle_id, old.bundle_id);
    }

    #[test]
    fn add_method_produces_n_plus_1_shares() {
        let old = build_initial_bundle(2, 3, 1);
        let kek = fixture_kek(29);
        let rotated = rotate_add_method(&old, &kek, vec![1]).unwrap();
        assert_eq!(rotated.shares.len(), 4);
        for share in &rotated.shares {
            assert_eq!(share.bundle_version, 2, "all shares carry new version");
        }
    }

    #[test]
    fn new_shares_combine_to_new_kek() {
        let old = build_initial_bundle(2, 3, 1);
        let new_kek = fixture_kek(31);
        let rotated = rotate_add_method(&old, &new_kek, vec![1]).unwrap();
        let recovered = combine_shares(2, &rotated.shares[..2]).unwrap();
        assert_eq!(recovered.as_bytes(), new_kek.as_bytes());
    }

    #[test]
    fn rotation_chain_links_correctly_across_three_rotations() {
        let v1 = build_initial_bundle(2, 3, 1);
        let kek_v2 = fixture_kek(37);
        let v2 = rotate_add_method(&v1, &kek_v2, vec![2]).unwrap().bundle;
        let kek_v3 = fixture_kek(41);
        let v3 = rotate_add_method(&v2, &kek_v3, vec![3]).unwrap().bundle;

        assert_eq!(v2.prior_bundle_id, Some(v1.bundle_id));
        assert_eq!(v3.prior_bundle_id, Some(v2.bundle_id));
        assert_eq!(v2.bundle_version, 2);
        assert_eq!(v3.bundle_version, 3);
        assert_eq!(v3.policy.threshold.n, 5, "3 -> 4 -> 5");
        assert_eq!(v3.policy.threshold.k, 2, "K stays put");
    }

    #[test]
    fn old_shares_cannot_recover_new_bundle_kek() {
        // Rollback defense: shares from v1 carry bundle_version=1.
        // The new bundle is at version 2 with shares carrying
        // bundle_version=2. Attempting to combine v1 shares against
        // the v2 bundle's key fails because combine_shares rejects
        // mixed-version sets.
        let old = build_initial_bundle(2, 3, 1);
        let old_kek = fixture_kek(2);
        let old_shares = split_kek(&old_kek, 2, 3, 1).unwrap();

        let new_kek = fixture_kek(43);
        let rotated = rotate_add_method(&old, &new_kek, vec![1]).unwrap();

        // Mix one old + one new share — rollback defense catches it.
        let mixed = vec![old_shares[0].clone(), rotated.shares[0].clone()];
        let result = combine_shares(2, &mixed);
        assert!(result.is_err());
    }

    // -- remove-method -------------------------------------------------

    #[test]
    fn remove_method_decrements_n_by_one() {
        let old = build_initial_bundle(2, 5, 1);
        let kek = fixture_kek(47);
        let rotated = rotate_remove_method(&old, &kek, vec![1]).unwrap();
        assert_eq!(rotated.bundle.policy.threshold.n, 4);
        assert_eq!(rotated.bundle.policy.threshold.k, 2);
    }

    #[test]
    fn remove_method_increments_bundle_version() {
        let old = build_initial_bundle(2, 5, 11);
        let kek = fixture_kek(53);
        let rotated = rotate_remove_method(&old, &kek, vec![1]).unwrap();
        assert_eq!(rotated.bundle.bundle_version, 12);
    }

    #[test]
    fn remove_method_chains_prior_bundle_id() {
        let old = build_initial_bundle(2, 5, 1);
        let kek = fixture_kek(59);
        let rotated = rotate_remove_method(&old, &kek, vec![1]).unwrap();
        assert_eq!(rotated.bundle.prior_bundle_id, Some(old.bundle_id));
    }

    #[test]
    fn remove_method_refuses_when_k_equals_n() {
        // 3-of-3 cannot become 3-of-2 — Threshold::new rejects K>N.
        let old = build_initial_bundle(3, 3, 1);
        let kek = fixture_kek(61);
        let result = rotate_remove_method(&old, &kek, vec![1]);
        assert!(matches!(result, Err(ShareError::InvalidThreshold { .. })));
    }

    #[test]
    fn remove_method_preserves_policy_constraints() {
        let old = build_initial_bundle(2, 5, 1);
        let kek = fixture_kek(67);
        let rotated = rotate_remove_method(&old, &kek, vec![1]).unwrap();
        assert_eq!(rotated.bundle.policy.constraints, old.policy.constraints);
    }

    #[test]
    fn remove_method_produces_n_minus_1_shares() {
        let old = build_initial_bundle(2, 5, 1);
        let kek = fixture_kek(71);
        let rotated = rotate_remove_method(&old, &kek, vec![1]).unwrap();
        assert_eq!(rotated.shares.len(), 4);
    }

    // -- rotate_credential --------------------------------------------

    #[test]
    fn rotate_credential_preserves_k_and_n() {
        let old = build_initial_bundle(3, 5, 7);
        let kek = fixture_kek(83);
        let rotated = rotate_credential(&old, &kek, vec![1]).unwrap();
        assert_eq!(rotated.bundle.policy.threshold.k, 3);
        assert_eq!(rotated.bundle.policy.threshold.n, 5);
    }

    #[test]
    fn rotate_credential_increments_bundle_version() {
        let old = build_initial_bundle(2, 3, 4);
        let kek = fixture_kek(89);
        let rotated = rotate_credential(&old, &kek, vec![1]).unwrap();
        assert_eq!(rotated.bundle.bundle_version, 5);
    }

    #[test]
    fn rotate_credential_chains_prior_bundle_id() {
        let old = build_initial_bundle(2, 3, 1);
        let kek = fixture_kek(97);
        let rotated = rotate_credential(&old, &kek, vec![1]).unwrap();
        assert_eq!(rotated.bundle.prior_bundle_id, Some(old.bundle_id));
    }

    #[test]
    fn rotate_credential_old_shares_cannot_recover_new_kek() {
        // Forward security: shares from the old bundle don't combine
        // for the new bundle's KEK.
        let old = build_initial_bundle(2, 3, 1);
        let old_kek = fixture_kek(101);
        let old_shares = split_kek(&old_kek, 2, 3, old.bundle_version).unwrap();
        let new_kek = fixture_kek(103);
        let rotated = rotate_credential(&old, &new_kek, vec![1]).unwrap();
        // Mix old + new — rejected.
        let mixed = vec![old_shares[0].clone(), rotated.shares[0].clone()];
        let result = combine_shares(2, &mixed);
        assert!(result.is_err());
    }

    #[test]
    fn rotate_credential_preserves_constraints() {
        let old = build_initial_bundle(2, 3, 1);
        let kek = fixture_kek(107);
        let rotated = rotate_credential(&old, &kek, vec![1]).unwrap();
        assert_eq!(rotated.bundle.policy.constraints, old.policy.constraints);
    }

    // -- rotate_change_threshold ---------------------------------------

    #[test]
    fn rotate_change_threshold_lowers_k() {
        let old = build_initial_bundle(3, 5, 1);
        let kek = fixture_kek(109);
        let rotated = rotate_change_threshold(&old, 2, &kek, vec![1]).unwrap();
        assert_eq!(rotated.bundle.policy.threshold.k, 2);
        assert_eq!(rotated.bundle.policy.threshold.n, 5, "N preserved");
    }

    #[test]
    fn rotate_change_threshold_raises_k() {
        let old = build_initial_bundle(2, 5, 1);
        let kek = fixture_kek(113);
        let rotated = rotate_change_threshold(&old, 4, &kek, vec![1]).unwrap();
        assert_eq!(rotated.bundle.policy.threshold.k, 4);
        assert_eq!(rotated.bundle.policy.threshold.n, 5);
    }

    #[test]
    fn rotate_change_threshold_refuses_k_greater_than_n() {
        let old = build_initial_bundle(2, 3, 1);
        let kek = fixture_kek(127);
        let result = rotate_change_threshold(&old, 4, &kek, vec![1]);
        assert!(matches!(result, Err(ShareError::InvalidThreshold { .. })));
    }

    #[test]
    fn rotate_change_threshold_refuses_zero_k() {
        let old = build_initial_bundle(2, 3, 1);
        let kek = fixture_kek(131);
        let result = rotate_change_threshold(&old, 0, &kek, vec![1]);
        assert!(matches!(result, Err(ShareError::InvalidThreshold { .. })));
    }

    #[test]
    fn rotate_change_threshold_increments_bundle_version() {
        let old = build_initial_bundle(2, 5, 7);
        let kek = fixture_kek(137);
        let rotated = rotate_change_threshold(&old, 3, &kek, vec![1]).unwrap();
        assert_eq!(rotated.bundle.bundle_version, 8);
    }

    #[test]
    fn change_threshold_required_quorum_takes_max() {
        // Lowering: K=3 -> K=2; required = max(3, 2) = 3.
        assert_eq!(change_threshold_required_quorum(3, 2), 3);
        // Raising: K=2 -> K=4; required = max(2, 4) = 4.
        assert_eq!(change_threshold_required_quorum(2, 4), 4);
        // Unchanged.
        assert_eq!(change_threshold_required_quorum(3, 3), 3);
    }

    // -- ciphertext is opaque to this layer ---------------------------

    #[test]
    fn rotation_carries_caller_supplied_ciphertext_unchanged() {
        let old = build_initial_bundle(2, 3, 1);
        let kek = fixture_kek(73);
        let new_ct = vec![0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe];
        let rotated = rotate_add_method(&old, &kek, new_ct.clone()).unwrap();
        assert_eq!(rotated.bundle.ciphertext, new_ct);
    }
}
