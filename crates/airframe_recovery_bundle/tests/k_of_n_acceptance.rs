//! K-of-N recovery acceptance test.
//!
//! Phase B Task 10 acceptance scenario: "Recovery from any K of N
//! methods reconstructs KEK; AEAD tag verifies bundle."
//!
//! This test exercises the full recovery primitive composition end-
//! to-end:
//!
//!   1. Build a fixture `BundlePayload` (install_id + master signing
//!      key material + CK).
//!   2. Generate a fixture KEK.
//!   3. AEAD-encrypt the payload bytes under the KEK with a random
//!      nonce (the nonce travels alongside the ciphertext).
//!   4. Wrap into a `RecoveryBundle` with a `RecoveryPolicy` that
//!      includes the full constraint suite.
//!   5. Split the KEK into N Shamir shares.
//!   6. Recovery: pick any K shares, combine to reconstruct the
//!      KEK, AEAD-decrypt the bundle ciphertext, deserialize the
//!      payload — bytes must match the original.
//!   7. Tamper detection: mutating any byte of the ciphertext, the
//!      KEK, or the AEAD tag must cause decryption to fail.
//!   8. Policy enforcement: `evaluate_with_enrollments` accepts the
//!      compliant method set and rejects sets that violate any
//!      constraint.
//!
//! # Why ChaCha20-Poly1305 here?
//!
//! This crate is documented as "format only — does not perform
//! AEAD". Production bundle wrapping happens in Nanokey under
//! Nanokey's chosen cipher (likely AES-GCM via OpenSSL). The
//! acceptance test needs *some* real AEAD to validate the data flow
//! end-to-end, so we use `chacha20poly1305` strictly as a dev-dep.
//! The test does not commit to a specific cipher in production.

use airframe_id::{AdminId, BundleId, InstallId, Threshold, TrusteeId};
use airframe_recovery_bundle::{
    combine_shares, rotate_add_method, rotate_remove_method, split_kek, BundleError, BundlePayload,
    Kek, MethodEnrollment, MethodRole, MethodRoleKind, MethodStatus, RecoveryBundle,
    RecoveryConstraint, RecoveryPolicy, ShareScheme, TrusteeRoleGroup,
};
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit, Nonce};
use rand::RngCore;

// ---------------------------------------------------------------------------
// AEAD helpers (test-only)
// ---------------------------------------------------------------------------

/// Bundle wire format used by these tests:
///
///   [12 bytes nonce | ciphertext-with-AEAD-tag]
///
/// The bundle's `ciphertext` field carries this concatenation. Real
/// Nanokey may pick a different layout — this is a test fixture.
fn aead_encrypt(kek: &Kek, plaintext: &[u8]) -> Vec<u8> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(kek.as_bytes()));
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher.encrypt(nonce, plaintext).expect("encrypt");
    let mut out = Vec::with_capacity(12 + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    out
}

#[derive(Debug)]
struct AeadFailed;

fn aead_decrypt(kek: &Kek, blob: &[u8]) -> Result<Vec<u8>, AeadFailed> {
    if blob.len() < 12 {
        return Err(AeadFailed);
    }
    let (nonce_bytes, ct) = blob.split_at(12);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(kek.as_bytes()));
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher.decrypt(nonce, ct).map_err(|_| AeadFailed)
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn fixture_payload() -> BundlePayload {
    BundlePayload {
        install_id: InstallId([0x42; 16]),
        master_signing_key_material: (0..64).map(|i| i as u8 * 3).collect(),
        ck: vec![0xab; 32],
    }
}

fn fixture_kek(seed: u8) -> Kek {
    let mut bytes = [0u8; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = (i as u8)
            .wrapping_mul(seed.wrapping_add(7))
            .wrapping_add(seed);
    }
    Kek(bytes)
}

fn admin_role(byte: u8) -> MethodRole {
    let mut bytes = [0u8; 16];
    bytes[0] = byte;
    MethodRole::AdminFactor {
        admin_id: AdminId(uuid::Uuid::from_bytes(bytes)),
    }
}

fn trustee_role(byte: u8, group: TrusteeRoleGroup) -> MethodRole {
    let mut bytes = [0u8; 16];
    bytes[0] = byte;
    MethodRole::ExternalTrustee {
        trustee_id: TrusteeId(uuid::Uuid::from_bytes(bytes)),
        role_group: group,
    }
}

fn org_role() -> MethodRole {
    MethodRole::OrgInfrastructure
}

/// Serialize a BundlePayload to bytes. Production uses a more
/// careful encoding; tests use serde_json since the fields are all
/// bytes-or-arrays-of-bytes and round-trip cleanly.
fn payload_bytes(p: &BundlePayload) -> Vec<u8> {
    // BundlePayload doesn't derive Serialize (it has zeroize on
    // sensitive fields), so we encode by hand.
    let mut out = Vec::new();
    out.extend_from_slice(&p.install_id.0);
    out.extend_from_slice(&(p.master_signing_key_material.len() as u32).to_le_bytes());
    out.extend_from_slice(&p.master_signing_key_material);
    out.extend_from_slice(&(p.ck.len() as u32).to_le_bytes());
    out.extend_from_slice(&p.ck);
    out
}

fn payload_from_bytes(bytes: &[u8]) -> BundlePayload {
    assert!(bytes.len() >= 16 + 4);
    let mut install = [0u8; 16];
    install.copy_from_slice(&bytes[..16]);
    let mut cursor = 16;
    let msk_len = u32::from_le_bytes([
        bytes[cursor],
        bytes[cursor + 1],
        bytes[cursor + 2],
        bytes[cursor + 3],
    ]) as usize;
    cursor += 4;
    let msk = bytes[cursor..cursor + msk_len].to_vec();
    cursor += msk_len;
    let ck_len = u32::from_le_bytes([
        bytes[cursor],
        bytes[cursor + 1],
        bytes[cursor + 2],
        bytes[cursor + 3],
    ]) as usize;
    cursor += 4;
    let ck = bytes[cursor..cursor + ck_len].to_vec();
    BundlePayload {
        install_id: InstallId(install),
        master_signing_key_material: msk,
        ck,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn k_of_n_round_trips_payload_via_aead() {
    let payload = fixture_payload();
    let kek = fixture_kek(7);
    let plaintext = payload_bytes(&payload);
    let ciphertext = aead_encrypt(&kek, &plaintext);

    let bundle = RecoveryBundle::new(
        1,
        None,
        RecoveryPolicy::threshold_only(Threshold::new(2, 3).unwrap()),
        ShareScheme::ShamirAeadV1,
        ciphertext.clone(),
    );

    let shares = split_kek(&kek, 2, 3, bundle.bundle_version).unwrap();
    assert_eq!(shares.len(), 3);

    // Recovery: any 2 of 3 shares.
    for (i, j) in [(0, 1), (0, 2), (1, 2)] {
        let recovered_kek = combine_shares(2, &[shares[i].clone(), shares[j].clone()]).unwrap();
        let decrypted = aead_decrypt(&recovered_kek, &bundle.ciphertext).expect("decrypt");
        let recovered = payload_from_bytes(&decrypted);
        assert_eq!(recovered.install_id, payload.install_id);
        assert_eq!(
            recovered.master_signing_key_material,
            payload.master_signing_key_material
        );
        assert_eq!(recovered.ck, payload.ck);
    }
}

#[test]
fn k_minus_one_shares_cannot_decrypt() {
    let payload = fixture_payload();
    let kek = fixture_kek(11);
    let ciphertext = aead_encrypt(&kek, &payload_bytes(&payload));
    let shares = split_kek(&kek, 3, 5, 1).unwrap();
    let result = combine_shares(3, &shares[..2]);
    assert!(
        result.is_err(),
        "K-1 shares must not reconstruct the KEK; combine_shares should error"
    );
    // Even if a buggy implementation returned an arbitrary 32 bytes,
    // AEAD verification would catch it.
    let bogus_kek = Kek([0u8; 32]);
    assert!(aead_decrypt(&bogus_kek, &ciphertext).is_err());
}

#[test]
fn tampered_ciphertext_fails_aead_verification() {
    let payload = fixture_payload();
    let kek = fixture_kek(13);
    let mut ciphertext = aead_encrypt(&kek, &payload_bytes(&payload));
    // Flip a byte in the ciphertext (skip the 12-byte nonce header
    // and pick a payload byte).
    ciphertext[20] ^= 0xff;
    let result = aead_decrypt(&kek, &ciphertext);
    assert!(result.is_err(), "tamper must surface as AEAD failure");
}

#[test]
fn tampered_aead_tag_fails_verification() {
    let payload = fixture_payload();
    let kek = fixture_kek(17);
    let mut ciphertext = aead_encrypt(&kek, &payload_bytes(&payload));
    // Flip the very last byte — that's inside the AEAD tag.
    let last = ciphertext.len() - 1;
    ciphertext[last] ^= 0xff;
    let result = aead_decrypt(&kek, &ciphertext);
    assert!(result.is_err());
}

#[test]
fn wrong_kek_fails_aead_verification() {
    let payload = fixture_payload();
    let kek = fixture_kek(19);
    let ciphertext = aead_encrypt(&kek, &payload_bytes(&payload));
    let other = fixture_kek(20);
    let result = aead_decrypt(&other, &ciphertext);
    assert!(result.is_err());
}

#[test]
fn full_acceptance_5_of_9_with_constraint_suite() {
    // The "real-world enterprise" pattern: 5-of-9 with at-least-one
    // external trustee + distinct trustee role groups + at-most-N
    // admin factors.
    let payload = fixture_payload();
    let kek = fixture_kek(23);
    let ciphertext = aead_encrypt(&kek, &payload_bytes(&payload));
    let policy = RecoveryPolicy {
        threshold: Threshold::new(5, 9).unwrap(),
        constraints: vec![
            RecoveryConstraint::AtLeastOneOfRole {
                role: MethodRoleKind::ExternalTrustee,
            },
            RecoveryConstraint::AtMostOneOfRole {
                role: MethodRoleKind::AdminFactor,
                max: 3,
            },
            RecoveryConstraint::DistinctTrusteeRoleGroups { minimum: 2 },
            RecoveryConstraint::DistinctHolders { minimum: 5 },
        ],
    };
    let bundle = RecoveryBundle::new(
        1,
        None,
        policy.clone(),
        ShareScheme::ShamirAeadV1,
        ciphertext.clone(),
    );

    let shares = split_kek(&kek, 5, 9, bundle.bundle_version).unwrap();
    assert_eq!(shares.len(), 9);

    // A compliant method set:
    //   3 admins + org infrastructure + 1 trustee from 1 distinct group =
    //     5 distinct holders, ≤3 admins, ≥1 trustee, but only 1 group...
    //   adjust: 2 admins + org + 2 trustees from distinct groups + 1 admin =
    //     5 distinct holders, 3 admins ✓, ≥1 trustee ✓, 2 groups ✓
    let methods = vec![
        MethodEnrollment {
            role: admin_role(1),
            status: MethodStatus::Active,
        },
        MethodEnrollment {
            role: admin_role(2),
            status: MethodStatus::Active,
        },
        MethodEnrollment {
            role: admin_role(3),
            status: MethodStatus::Active,
        },
        MethodEnrollment {
            role: org_role(),
            status: MethodStatus::Active,
        },
        MethodEnrollment {
            role: trustee_role(1, TrusteeRoleGroup::Personal),
            status: MethodStatus::Active,
        },
    ];
    let (eval, eff) = policy.evaluate_with_enrollments(&methods);
    // This particular set has only 1 trustee group, so DistinctTrustees
    // fails. That's the point: assert the rejection.
    assert!(eval.is_err(), "single-group trustee fails distinctness");
    assert_eq!(eff.effective_n, 5);

    // Now satisfy DistinctTrusteeRoleGroups by adding a second-group trustee.
    let compliant = vec![
        MethodEnrollment {
            role: admin_role(1),
            status: MethodStatus::Active,
        },
        MethodEnrollment {
            role: admin_role(2),
            status: MethodStatus::Active,
        },
        MethodEnrollment {
            role: admin_role(3),
            status: MethodStatus::Active,
        },
        MethodEnrollment {
            role: trustee_role(1, TrusteeRoleGroup::Personal),
            status: MethodStatus::Active,
        },
        MethodEnrollment {
            role: trustee_role(2, TrusteeRoleGroup::Executive),
            status: MethodStatus::Active,
        },
    ];
    let (eval, _) = policy.evaluate_with_enrollments(&compliant);
    assert!(eval.is_ok(), "compliant set passes all four constraints");

    // KEK reconstruction with K=5 shares.
    let recovered = combine_shares(5, &shares[..5]).unwrap();
    let plaintext = aead_decrypt(&recovered, &bundle.ciphertext).expect("decrypt");
    let recovered_payload = payload_from_bytes(&plaintext);
    assert_eq!(recovered_payload.install_id, payload.install_id);
}

#[test]
fn provisional_methods_dont_count_toward_recovery_threshold() {
    // Bootstrap committed with some methods Provisional. The
    // effective threshold reduces, but the bundle ciphertext was
    // encrypted under a KEK whose Shamir split assumed all N
    // methods are share-bearing.
    //
    // For v1 the share split happens at bootstrap on Active+
    // PendingRemoval methods only; Provisional methods don't get
    // shares yet (they get them on promotion). Test that the
    // policy evaluator agrees.
    let policy = RecoveryPolicy {
        threshold: Threshold::new(3, 5).unwrap(),
        constraints: vec![],
    };
    let methods = vec![
        MethodEnrollment {
            role: admin_role(1),
            status: MethodStatus::Active,
        },
        MethodEnrollment {
            role: admin_role(2),
            status: MethodStatus::Active,
        },
        // 3 provisional methods don't count.
        MethodEnrollment {
            role: admin_role(3),
            status: MethodStatus::Provisional {
                expected_by_unix: 0,
            },
        },
        MethodEnrollment {
            role: org_role(),
            status: MethodStatus::Provisional {
                expected_by_unix: 0,
            },
        },
        MethodEnrollment {
            role: trustee_role(1, TrusteeRoleGroup::Personal),
            status: MethodStatus::Provisional {
                expected_by_unix: 0,
            },
        },
    ];
    let (eval, eff) = policy.evaluate_with_enrollments(&methods);
    // Effective N=2, K clamped to 2 (from declared 3).
    assert_eq!(eff.effective_n, 2);
    assert_eq!(eff.effective_k, 2);
    assert!(
        eval.is_ok(),
        "the 2 active methods satisfy the clamped 2-of-2"
    );
}

#[test]
fn rollback_defense_mixed_bundle_versions_fail_recovery() {
    let payload = fixture_payload();
    let kek = fixture_kek(29);
    let ct = aead_encrypt(&kek, &payload_bytes(&payload));
    let _bundle = RecoveryBundle::new(
        2,
        Some(BundleId::new()),
        RecoveryPolicy::threshold_only(Threshold::new(2, 3).unwrap()),
        ShareScheme::ShamirAeadV1,
        ct,
    );

    // Mint shares for v1 AND v2; mixing them must fail.
    let mut shares = split_kek(&kek, 2, 3, 1).unwrap();
    let v2_shares = split_kek(&kek, 2, 3, 2).unwrap();
    shares[1] = v2_shares[1].clone();

    let result = combine_shares(2, &shares[..2]);
    assert!(
        result.is_err(),
        "mixed-version share set must surface as MalformedShare"
    );
}

#[test]
fn aead_round_trips_through_persisted_bundle_envelope() {
    // Pin the integration: build a bundle, serialize the envelope,
    // round-trip through JSON, deserialize, decrypt with K shares.
    // This is the storage path Nanokey's bundle CRUD endpoint exercises.
    let payload = fixture_payload();
    let kek = fixture_kek(31);
    let ct = aead_encrypt(&kek, &payload_bytes(&payload));
    let bundle = RecoveryBundle::new(
        7,
        None,
        RecoveryPolicy::threshold_only(Threshold::new(2, 3).unwrap()),
        ShareScheme::ShamirAeadV1,
        ct,
    );
    let shares = split_kek(&kek, 2, 3, bundle.bundle_version).unwrap();

    // Serialize the envelope (matches the wire shape Nanokey persists).
    let json = serde_json::to_vec(&bundle).unwrap();
    let restored: RecoveryBundle = serde_json::from_slice(&json).unwrap();
    assert_eq!(restored.bundle_version, 7);

    // Recovery from the deserialized envelope.
    let recovered = combine_shares(2, &shares[..2]).unwrap();
    let plaintext = aead_decrypt(&recovered, &restored.ciphertext).expect("decrypt");
    let recovered_payload = payload_from_bytes(&plaintext);
    assert_eq!(recovered_payload.install_id, payload.install_id);
    assert_eq!(recovered_payload.ck, payload.ck);
}

#[test]
fn add_method_rotation_full_aead_round_trip() {
    // Phase B Task 11 acceptance: adding a method increments bundle
    // version, re-Shamirs to (K, N+1), and the new shares decrypt
    // the new ciphertext under the freshly-issued KEK.
    let payload = fixture_payload();
    let v1_kek = fixture_kek(101);
    let v1_ct = aead_encrypt(&v1_kek, &payload_bytes(&payload));
    let v1 = RecoveryBundle::new(
        1,
        None,
        RecoveryPolicy::threshold_only(Threshold::new(2, 3).unwrap()),
        ShareScheme::ShamirAeadV1,
        v1_ct,
    );
    // Operator simulates adding a method: recovers payload via K shares,
    // mints a fresh KEK, re-encrypts the SAME payload under it.
    let v1_shares = split_kek(&v1_kek, 2, 3, v1.bundle_version).unwrap();
    let recovered_kek = combine_shares(2, &v1_shares[..2]).unwrap();
    let recovered_payload = aead_decrypt(&recovered_kek, &v1.ciphertext).expect("decrypt v1");
    let v2_kek = fixture_kek(102);
    let v2_ct = aead_encrypt(&v2_kek, &recovered_payload);

    let rotated = rotate_add_method(&v1, &v2_kek, v2_ct).unwrap();
    assert_eq!(rotated.bundle.bundle_version, 2);
    assert_eq!(rotated.bundle.policy.threshold.k, 2);
    assert_eq!(rotated.bundle.policy.threshold.n, 4);
    assert_eq!(rotated.shares.len(), 4);

    // Recovery from the new bundle works under the new K shares.
    let new_recovered_kek = combine_shares(2, &rotated.shares[..2]).unwrap();
    assert_eq!(new_recovered_kek.as_bytes(), v2_kek.as_bytes());
    let plaintext =
        aead_decrypt(&new_recovered_kek, &rotated.bundle.ciphertext).expect("decrypt v2");
    let restored = payload_from_bytes(&plaintext);
    assert_eq!(restored.install_id, payload.install_id);
    assert_eq!(restored.ck, payload.ck);
    assert_eq!(
        restored.master_signing_key_material,
        payload.master_signing_key_material
    );

    // Old shares cannot decrypt the new bundle (rollback defense).
    let bad_kek_attempt = combine_shares(2, &v1_shares[..2]).unwrap();
    assert!(
        aead_decrypt(&bad_kek_attempt, &rotated.bundle.ciphertext).is_err(),
        "v1 KEK must not decrypt v2 ciphertext"
    );
}

#[test]
fn remove_method_rotation_full_aead_round_trip() {
    // Phase B Task 12 acceptance: removing a method (possessed)
    // re-Shamirs to (K, N-1) and the new shares decrypt the new
    // ciphertext.
    let payload = fixture_payload();
    let v1_kek = fixture_kek(103);
    let v1_ct = aead_encrypt(&v1_kek, &payload_bytes(&payload));
    let v1 = RecoveryBundle::new(
        5,
        None,
        RecoveryPolicy::threshold_only(Threshold::new(2, 5).unwrap()),
        ShareScheme::ShamirAeadV1,
        v1_ct,
    );
    let v1_shares = split_kek(&v1_kek, 2, 5, v1.bundle_version).unwrap();
    let recovered_kek = combine_shares(2, &v1_shares[..2]).unwrap();
    let recovered_payload = aead_decrypt(&recovered_kek, &v1.ciphertext).expect("decrypt");
    let v2_kek = fixture_kek(104);
    let v2_ct = aead_encrypt(&v2_kek, &recovered_payload);

    let rotated = rotate_remove_method(&v1, &v2_kek, v2_ct).unwrap();
    assert_eq!(rotated.bundle.bundle_version, 6);
    assert_eq!(rotated.bundle.policy.threshold.n, 4);
    assert_eq!(rotated.shares.len(), 4);

    let new_recovered_kek = combine_shares(2, &rotated.shares[..2]).unwrap();
    let plaintext =
        aead_decrypt(&new_recovered_kek, &rotated.bundle.ciphertext).expect("decrypt v2");
    let restored = payload_from_bytes(&plaintext);
    assert_eq!(restored.install_id, payload.install_id);
}

#[test]
fn policy_constraint_rejection_does_not_prevent_kek_reconstruction() {
    // The constraint evaluator gates *whether* recovery should
    // proceed, not *whether the math works*. The Shamir math is
    // independent. This test pins that distinction so a future
    // refactor doesn't accidentally couple the two layers.
    let payload = fixture_payload();
    let kek = fixture_kek(37);
    let ct = aead_encrypt(&kek, &payload_bytes(&payload));
    let bundle = RecoveryBundle::new(
        1,
        None,
        RecoveryPolicy {
            threshold: Threshold::new(2, 3).unwrap(),
            constraints: vec![RecoveryConstraint::AtLeastOneOfRole {
                role: MethodRoleKind::ExternalTrustee,
            }],
        },
        ShareScheme::ShamirAeadV1,
        ct,
    );
    let shares = split_kek(&kek, 2, 3, bundle.bundle_version).unwrap();

    // Method set violates the constraint (no trustee).
    let methods = vec![
        MethodEnrollment {
            role: admin_role(1),
            status: MethodStatus::Active,
        },
        MethodEnrollment {
            role: admin_role(2),
            status: MethodStatus::Active,
        },
    ];
    let (eval, _) = bundle.policy.evaluate_with_enrollments(&methods);
    assert!(matches!(eval, Err(BundleError::ConstraintFailed(_))));

    // But the math still works — operator can see "shares combine,
    // payload decrypts, but policy refuses". That's the surface
    // the recovery ceremony relies on for clear error reporting.
    let recovered = combine_shares(2, &shares[..2]).unwrap();
    let plaintext = aead_decrypt(&recovered, &bundle.ciphertext).expect("decrypt");
    let recovered_payload = payload_from_bytes(&plaintext);
    assert_eq!(recovered_payload.install_id, payload.install_id);
}
