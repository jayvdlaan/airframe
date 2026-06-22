//! Shared identifier types for the Airframe ceremony framework.
//!
//! These types are used across the framework, recovery system, and
//! cross-service orchestration. They're deliberately minimal — newtype
//! wrappers around `Uuid` or fixed-size byte arrays — so they can be
//! depended on by every layer without pulling in heavier abstractions.
//!
//! See `docs/ref-ceremony-types.md` in the airspace repo for the
//! authoritative specification.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroize;

/// Identifier for a ceremony instance. Unique per install.
///
/// Generated when a ceremony starts; persisted in the ceremony header
/// at `np:ceremonies:{id}` (or `nk:ceremonies:{id}`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct CeremonyId(pub Uuid);

impl CeremonyId {
    /// Generate a new random `CeremonyId`.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for CeremonyId {
    fn default() -> Self {
        Self::new()
    }
}

/// 128-bit shared identity between Nanokey and Nanopass.
///
/// Established once at first bootstrap and preserved across recovery.
/// The load-bearing identity for foot-gun guards (Nanopass refuses to
/// connect to a Nanokey reporting a different `InstallId`) and for
/// audit correlation.
///
/// Stored at `nk:install_id` (Nanokey-owned) and mirrored at
/// `np:install_id` (Nanopass) for the foot-gun check.
// `Zeroize` is derived because downstream secret-bearing structs (e.g. in
// airframe_recovery_bundle) embed an `InstallId` and rely on it being zeroizable
// for their own `ZeroizeOnDrop`. As a standalone `Copy` value zeroizing is of
// limited effect, but as a wiped field it is meaningful.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, Zeroize)]
pub struct InstallId(pub [u8; 16]);

impl InstallId {
    /// Generate a new random `InstallId` from a UUID v4 (also 128 bits).
    ///
    /// Use only at first bootstrap. Must NEVER be regenerated for an
    /// existing install — recovery preserves the original.
    pub fn new() -> Self {
        Self(*Uuid::new_v4().as_bytes())
    }

    /// Render as a hex-with-dashes form for human display: `4f3a-1c92-71b3-0d22-...`.
    pub fn display_short(&self) -> String {
        let h = hex_lower(&self.0);
        // Group as 4-char chunks separated by `-` for readability.
        let mut s = String::with_capacity(h.len() + h.len() / 4);
        for (i, c) in h.chars().enumerate() {
            if i > 0 && i % 4 == 0 {
                s.push('-');
            }
            s.push(c);
        }
        s
    }
}

impl Default for InstallId {
    fn default() -> Self {
        Self::new()
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// Identifier for an enrolled recovery method.
///
/// Persisted at `np:recovery_methods:{method_id}` or equivalent.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct MethodId(pub Uuid);

impl MethodId {
    /// Generate a new random `MethodId`.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MethodId {
    fn default() -> Self {
        Self::new()
    }
}

/// Identifier for a recovery bundle revision.
///
/// Each bundle rotation produces a new `BundleId`. The bundle's
/// `prior_bundle_id` field forms a chain of history.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct BundleId(pub Uuid);

impl BundleId {
    /// Generate a new random `BundleId`.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for BundleId {
    fn default() -> Self {
        Self::new()
    }
}

/// Identifier for a named administrator.
///
/// First-class identity; not just a method holder. Persisted at
/// `np:admins:{admin_id}`. Status changes (Active → Suspended → Removed)
/// are tracked in the admin's record.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct AdminId(pub Uuid);

impl AdminId {
    /// Generate a new random `AdminId`.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AdminId {
    fn default() -> Self {
        Self::new()
    }
}

/// Identifier for an external recovery trustee.
///
/// Distinct from `AdminId` — trustees are outside the org boundary
/// and may not interact with the application beyond holding their share.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct TrusteeId(pub Uuid);

impl TrusteeId {
    /// Generate a new random `TrusteeId`.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TrusteeId {
    fn default() -> Self {
        Self::new()
    }
}

/// K-of-N threshold over enrolled methods or trustees.
///
/// `K=1` is OR-recovery (any one method satisfies); `K=N` is AND-recovery
/// (all methods required); `1 < K < N` is the typical case.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Threshold {
    /// Minimum number of methods/shares required to reconstruct.
    pub k: u32,
    /// Total number of methods/shares enrolled.
    pub n: u32,
}

impl Threshold {
    /// Construct a threshold, returning `None` if invalid (k > n or k == 0).
    pub fn new(k: u32, n: u32) -> Option<Self> {
        if k == 0 || k > n {
            None
        } else {
            Some(Self { k, n })
        }
    }

    /// Returns true if this threshold could be satisfied by the given
    /// number of available methods.
    pub fn satisfiable_by(&self, available: u32) -> bool {
        available >= self.k
    }

    /// True if this is an "any one of N" threshold (K=1).
    pub fn is_or(&self) -> bool {
        self.k == 1
    }

    /// True if this is an "all of N" threshold (K=N).
    pub fn is_unanimous(&self) -> bool {
        self.k == self.n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_id_round_trips_through_serde_json() {
        let id = InstallId([
            0x4f, 0x3a, 0x1c, 0x92, 0x71, 0xb3, 0x0d, 0x22, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45,
            0x67, 0x89,
        ]);
        let json = serde_json::to_string(&id).unwrap();
        let back: InstallId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn install_id_display_short_groups_in_fours() {
        let id = InstallId([
            0x4f, 0x3a, 0x1c, 0x92, 0x71, 0xb3, 0x0d, 0x22, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45,
            0x67, 0x89,
        ]);
        // 32 hex chars in groups of 4 with dashes between — 7 dashes total.
        assert_eq!(
            id.display_short(),
            "4f3a-1c92-71b3-0d22-abcd-ef01-2345-6789"
        );
    }

    #[test]
    fn ceremony_id_round_trips() {
        let id = CeremonyId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: CeremonyId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn method_bundle_admin_trustee_ids_round_trip() {
        let m = MethodId::new();
        let b = BundleId::new();
        let a = AdminId::new();
        let t = TrusteeId::new();
        for (label, json) in [
            ("MethodId", serde_json::to_string(&m).unwrap()),
            ("BundleId", serde_json::to_string(&b).unwrap()),
            ("AdminId", serde_json::to_string(&a).unwrap()),
            ("TrusteeId", serde_json::to_string(&t).unwrap()),
        ] {
            assert!(
                json.contains('-'),
                "{label} should serialize as a string with dashes (uuid form): got {json}"
            );
        }
        // Round-trip preserves equality.
        assert_eq!(
            m,
            serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap()
        );
        assert_eq!(
            b,
            serde_json::from_str(&serde_json::to_string(&b).unwrap()).unwrap()
        );
        assert_eq!(
            a,
            serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap()
        );
        assert_eq!(
            t,
            serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap()
        );
    }

    #[test]
    fn threshold_construction_validates() {
        assert_eq!(Threshold::new(2, 3), Some(Threshold { k: 2, n: 3 }));
        assert_eq!(Threshold::new(0, 3), None, "k=0 must be invalid");
        assert_eq!(Threshold::new(4, 3), None, "k>n must be invalid");
        assert_eq!(Threshold::new(3, 3), Some(Threshold { k: 3, n: 3 }));
    }

    #[test]
    fn threshold_predicates() {
        let t = Threshold::new(2, 3).unwrap();
        assert!(t.satisfiable_by(2));
        assert!(t.satisfiable_by(3));
        assert!(!t.satisfiable_by(1));
        assert!(!t.is_or());
        assert!(!t.is_unanimous());

        assert!(Threshold::new(1, 5).unwrap().is_or());
        assert!(Threshold::new(5, 5).unwrap().is_unanimous());
    }

    #[test]
    fn threshold_round_trips() {
        let t = Threshold::new(3, 5).unwrap();
        let json = serde_json::to_string(&t).unwrap();
        let back: Threshold = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn ids_are_unique_across_calls() {
        let a = CeremonyId::new();
        let b = CeremonyId::new();
        assert_ne!(a, b, "two new CeremonyIds collided — RNG broken?");

        let i = InstallId::new();
        let j = InstallId::new();
        assert_ne!(i, j, "two new InstallIds collided — RNG broken?");
    }
}
