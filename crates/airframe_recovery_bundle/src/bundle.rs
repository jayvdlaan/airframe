//! Recovery bundle: the encrypted artifact users save to recover from
//! catastrophic loss.

use std::time::SystemTime;

use airframe_id::{BundleId, InstallId};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::policy::{RecoveryPolicy, ShareScheme};

/// 256-bit Key Encryption Key (KEK).
///
/// The KEK is the secret that's split via Shamir into shares; combining K
/// shares reconstructs the KEK; the KEK then unwraps the bundle's
/// `ciphertext` (via AEAD outside this crate's scope).
///
/// Implements [`Zeroize`] / [`ZeroizeOnDrop`] so the bytes are wiped from
/// memory when the value goes out of scope.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct Kek(pub [u8; 32]);

impl Kek {
    /// View the underlying bytes. Caller is responsible for not copying
    /// out of this view in a way that defeats zeroization.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for Kek {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the actual bytes.
        write!(f, "Kek(<redacted>)")
    }
}

/// The recovery bundle as persisted at `np:recovery_bundle` (Nanopass) or
/// `nk:recovery_bundle` (Nanokey).
///
/// Contains cleartext metadata (which Shamir scheme, which version,
/// chain-of-history) plus an opaque ciphertext blob that this crate does
/// not decrypt — that's the caller's responsibility.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecoveryBundle {
    /// Identifier for this bundle revision.
    pub bundle_id: BundleId,
    /// Monotonic version; increments on every rotation.
    pub bundle_version: u32,
    /// Reference to the previous bundle in the chain, or `None` for the
    /// first bundle minted at install bootstrap.
    pub prior_bundle_id: Option<BundleId>,
    /// Threshold + role-mix constraints for recovery.
    pub policy: RecoveryPolicy,
    /// Which scheme produced this bundle's shares.
    pub scheme: ShareScheme,
    /// Opaque AEAD ciphertext of the bundle plaintext. The plaintext
    /// structure (after decryption) is [`BundlePayload`].
    pub ciphertext: Vec<u8>,
    /// When the bundle was created. Unix seconds.
    pub created_at_unix: u64,
}

impl RecoveryBundle {
    /// Create a new bundle metadata record. The ciphertext is provided
    /// by the caller (this crate doesn't perform AEAD).
    pub fn new(
        bundle_version: u32,
        prior_bundle_id: Option<BundleId>,
        policy: RecoveryPolicy,
        scheme: ShareScheme,
        ciphertext: Vec<u8>,
    ) -> Self {
        let created_at_unix = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            bundle_id: BundleId::new(),
            bundle_version,
            prior_bundle_id,
            policy,
            scheme,
            ciphertext,
            created_at_unix,
        }
    }
}

/// What's inside the bundle when decrypted with K methods.
///
/// Note: does NOT contain `sealed_decisions`, audience, or per-deployment
/// config. Witnesses are recomputed during recovery from current TOML
/// against the recovered `master_signing_key_material`. This keeps the
/// bundle minimal — one artifact, one threat model — and lets recovery
/// proceed even if the wizard's plan-template configuration evolved
/// between bundle creation and recovery.
///
/// All fields are sensitive. Implements [`Zeroize`] / [`ZeroizeOnDrop`]
/// so the bytes are wiped from memory when the value goes out of scope.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct BundlePayload {
    /// The install's 128-bit shared identity. Preserved across recovery.
    pub install_id: InstallId,
    /// Master signing key material used by Nanokey to sign witnesses
    /// and audit entries. Stored as opaque bytes — the format is
    /// determined by Nanokey's choice of signing algorithm.
    pub master_signing_key_material: Vec<u8>,
    /// The Cache Key (CK) that wraps every keystore DEK. 32 bytes.
    pub ck: Vec<u8>,
}

impl std::fmt::Debug for BundlePayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print secret bytes.
        f.debug_struct("BundlePayload")
            .field("install_id", &self.install_id)
            .field("master_signing_key_material", &"<redacted>")
            .field("ck", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{RecoveryConstraint, RecoveryPolicy};
    use airframe_id::Threshold;

    #[test]
    fn recovery_bundle_round_trips() {
        let bundle = RecoveryBundle::new(
            1,
            None,
            RecoveryPolicy::threshold_only(Threshold::new(2, 3).unwrap()),
            ShareScheme::ShamirAeadV1,
            vec![0xde, 0xad, 0xbe, 0xef],
        );
        let json = serde_json::to_string(&bundle).unwrap();
        let back: RecoveryBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(bundle.bundle_id, back.bundle_id);
        assert_eq!(bundle.bundle_version, back.bundle_version);
        assert_eq!(bundle.policy, back.policy);
        assert_eq!(bundle.ciphertext, back.ciphertext);
    }

    #[test]
    fn bundle_with_constraints_round_trips() {
        let policy = RecoveryPolicy {
            threshold: Threshold::new(2, 5).unwrap(),
            constraints: vec![
                RecoveryConstraint::AtLeastOneOfRole {
                    role: crate::policy::MethodRoleKind::ExternalTrustee,
                },
                RecoveryConstraint::DistinctTrusteeRoleGroups { minimum: 2 },
            ],
        };
        let bundle = RecoveryBundle::new(
            7,
            Some(BundleId::new()),
            policy,
            ShareScheme::ShamirAeadV1,
            vec![1, 2, 3, 4, 5],
        );
        let json = serde_json::to_string(&bundle).unwrap();
        let back: RecoveryBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(bundle.policy, back.policy);
    }

    #[test]
    fn kek_debug_does_not_leak_bytes() {
        let kek = Kek([0xff; 32]);
        let s = format!("{kek:?}");
        assert_eq!(s, "Kek(<redacted>)");
        assert!(!s.contains("ff"));
    }

    #[test]
    fn bundle_payload_debug_does_not_leak_secrets() {
        let payload = BundlePayload {
            install_id: InstallId([0; 16]),
            master_signing_key_material: vec![0xaa; 32],
            ck: vec![0xbb; 32],
        };
        let s = format!("{payload:?}");
        assert!(s.contains("install_id"));
        assert!(s.contains("redacted"));
        assert!(!s.contains("aa"));
        assert!(!s.contains("bb"));
    }

    // ─────────────────────────────────────────────────────────────────
    // Tier A forward-compat regression tests.
    //
    // Pins the discipline that lets v1.0 (Tier A) bundles remain
    // readable when future versions grow the envelope schema. If a
    // future change breaks either of these contracts, an install
    // bootstrapped on v1.0 becomes unrecoverable after upgrading —
    // a catastrophic failure mode worth a loud test.
    //
    // The cleartext `BundlePayload` is serialized by a wrapper type
    // (`PersistedBundlePayload` in `nanopass_bootstrap::recovery`)
    // because `BundlePayload` itself is `Zeroize`/`ZeroizeOnDrop` and
    // doesn't impl `Serialize`/`Deserialize`. Its wire schema is
    // owned by that wrapper and tested there.
    // ─────────────────────────────────────────────────────────────────

    /// A bundle minted with `bundle_version: 1` (the v1.0 shape)
    /// round-trips through JSON unchanged. v1.1+ readers MUST be
    /// able to decode this exact shape.
    #[test]
    fn v1_bundle_envelope_round_trips() {
        let bundle = RecoveryBundle::new(
            1,
            None,
            RecoveryPolicy::threshold_only(Threshold::new(1, 1).unwrap()),
            ShareScheme::ShamirAeadV1,
            vec![0xca, 0xfe, 0xba, 0xbe],
        );
        let json = serde_json::to_string(&bundle).expect("serialize");
        let back: RecoveryBundle = serde_json::from_str(&json).expect("v1 bundle must deserialize");
        assert_eq!(back.bundle_version, 1);
        assert_eq!(back.bundle_id, bundle.bundle_id);
        assert_eq!(back.policy, bundle.policy);
        assert_eq!(back.ciphertext, bundle.ciphertext);
    }

    /// A bundle JSON CARRYING an unknown future field at the envelope
    /// level still deserializes — serde's `deny_unknown_fields` is NOT
    /// in effect so v1.0 can read v1.1+ envelopes. If you ever add
    /// `#[serde(deny_unknown_fields)]` to `RecoveryBundle`, this test
    /// will catch it.
    #[test]
    fn future_bundle_envelope_with_unknown_field_decodes() {
        let json = r#"{
            "bundle_id": "00000000-0000-0000-0000-000000000001",
            "bundle_version": 2,
            "prior_bundle_id": null,
            "policy": {
                "threshold": {"k": 1, "n": 1},
                "constraints": []
            },
            "scheme": "ShamirAeadV1",
            "ciphertext": [222, 173, 190, 239],
            "created_at_unix": 100,
            "future_field": "would only exist in v1.1+"
        }"#;
        let _back: RecoveryBundle =
            serde_json::from_str(json).expect("envelope with extra field must decode");
    }
}
