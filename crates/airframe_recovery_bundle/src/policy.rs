//! Recovery policy: threshold + role-mix constraints.

use airframe_id::{AdminId, Threshold, TrusteeId};
use serde::{Deserialize, Serialize};

use crate::error::BundleError;

/// Recovery policy: a K-of-N threshold plus optional role-mix constraints
/// that all must be satisfied for recovery to succeed.
///
/// Recovery succeeds when the user provides K methods that satisfy *both*
/// the threshold *and* every constraint in [`RecoveryPolicy::constraints`].
///
/// See `docs/arch-recovery-system.md` "RecoveryPolicy" section for the
/// authoritative specification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryPolicy {
    /// K-of-N threshold over enrolled methods.
    pub threshold: Threshold,
    /// Additional constraints that must all hold over the methods used.
    pub constraints: Vec<RecoveryConstraint>,
}

impl RecoveryPolicy {
    /// Construct a policy with no constraints (threshold-only recovery).
    pub fn threshold_only(threshold: Threshold) -> Self {
        Self {
            threshold,
            constraints: Vec::new(),
        }
    }

    /// Evaluate whether the given set of method roles satisfies this policy.
    ///
    /// Returns `Ok(())` if both threshold and all constraints are met,
    /// otherwise the specific failure.
    pub fn evaluate(&self, methods_used: &[MethodRole]) -> Result<(), BundleError> {
        let count = methods_used.len() as u32;
        if count < self.threshold.k {
            return Err(BundleError::ConstraintFailed(format!(
                "threshold not met: need {} methods, got {}",
                self.threshold.k, count
            )));
        }
        for c in &self.constraints {
            c.evaluate(methods_used)?;
        }
        Ok(())
    }
}

/// A constraint on which methods may participate in recovery.
///
/// Constraints prevent bypass scenarios in multi-admin installs. For
/// example, `AtLeastOneOfRole(ExternalTrustee)` prevents a sole-survivor
/// admin from unilateral recovery using only internal methods.
///
/// Serialized in internally-tagged form (`{ kind: AtLeastOneOfRole, role: ExternalTrustee }`)
/// to match the YAML plan-template schema.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum RecoveryConstraint {
    /// At least one method used must be tagged with the given role kind.
    AtLeastOneOfRole {
        /// The role kind that must appear at least once.
        role: MethodRoleKind,
    },
    /// At most `max` methods of the given role kind may be used.
    AtMostOneOfRole {
        /// The role kind being capped.
        role: MethodRoleKind,
        /// Maximum allowed count of methods with this role.
        max: u32,
    },
    /// Methods used must come from at least `minimum` distinct holders.
    DistinctHolders {
        /// Minimum number of distinct holders.
        minimum: u32,
    },
    /// Trustee methods used must span at least `minimum` distinct
    /// [`TrusteeRoleGroup`]s. Prevents single-cluster takeover.
    DistinctTrusteeRoleGroups {
        /// Minimum number of distinct role groups.
        minimum: u32,
    },
}

impl RecoveryConstraint {
    fn evaluate(&self, methods: &[MethodRole]) -> Result<(), BundleError> {
        match self {
            Self::AtLeastOneOfRole { role } => {
                let has = methods.iter().any(|m| m.kind() == *role);
                if has {
                    Ok(())
                } else {
                    Err(BundleError::ConstraintFailed(format!(
                        "AtLeastOneOfRole({role:?}) not satisfied"
                    )))
                }
            }
            Self::AtMostOneOfRole { role, max } => {
                let count = methods.iter().filter(|m| m.kind() == *role).count() as u32;
                if count <= *max {
                    Ok(())
                } else {
                    Err(BundleError::ConstraintFailed(format!(
                        "AtMostOneOfRole({role:?}, max={max}) violated: count={count}"
                    )))
                }
            }
            Self::DistinctHolders { minimum } => {
                let distinct: std::collections::HashSet<_> = methods.iter().collect();
                let n = distinct.len() as u32;
                if n >= *minimum {
                    Ok(())
                } else {
                    Err(BundleError::ConstraintFailed(format!(
                        "DistinctHolders(min={minimum}) not satisfied: only {n} distinct"
                    )))
                }
            }
            Self::DistinctTrusteeRoleGroups { minimum } => {
                let groups: std::collections::HashSet<_> = methods
                    .iter()
                    .filter_map(|m| match m {
                        MethodRole::ExternalTrustee { role_group, .. } => Some(*role_group),
                        _ => None,
                    })
                    .collect();
                let n = groups.len() as u32;
                if n >= *minimum {
                    Ok(())
                } else {
                    Err(BundleError::ConstraintFailed(format!(
                        "DistinctTrusteeRoleGroups(min={minimum}) not satisfied: only {n} groups"
                    )))
                }
            }
        }
    }
}

/// A method's tagged role for constraint purposes.
///
/// Distinct from `FactorRole` (Primary/Recovery in the framework crate);
/// this is about *who holds* the method organizationally.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum MethodRole {
    /// Method belongs to a named admin.
    AdminFactor {
        /// The admin's identity.
        admin_id: AdminId,
    },
    /// Method belongs to org infrastructure (org cloud account, TPM, etc.).
    OrgInfrastructure,
    /// Method belongs to an external trustee (outside the org boundary).
    ExternalTrustee {
        /// The trustee's identity.
        trustee_id: TrusteeId,
        /// Which trustee role group they belong to.
        role_group: TrusteeRoleGroup,
    },
    /// Single-user installs; no admin/trustee distinction.
    SystemFactor,
}

impl MethodRole {
    /// Returns the coarse [`MethodRoleKind`] for constraint matching.
    pub fn kind(&self) -> MethodRoleKind {
        match self {
            Self::AdminFactor { .. } => MethodRoleKind::AdminFactor,
            Self::OrgInfrastructure => MethodRoleKind::OrgInfrastructure,
            Self::ExternalTrustee { .. } => MethodRoleKind::ExternalTrustee,
            Self::SystemFactor => MethodRoleKind::SystemFactor,
        }
    }
}

/// Coarser role tag for constraint expressions; matches against
/// [`MethodRole`] without binding to a specific admin or trustee identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum MethodRoleKind {
    /// Admin-held methods.
    AdminFactor,
    /// Org-infrastructure methods.
    OrgInfrastructure,
    /// External-trustee methods.
    ExternalTrustee,
    /// System (single-user) methods.
    SystemFactor,
}

/// Trustee-specific refinement of [`MethodRole::ExternalTrustee`].
///
/// Used by recovery role-mix constraints to prevent a single social
/// cluster from unilateral takeover.
///
/// The taxonomy:
/// - **Personal** — family, partners, close friends. Strong personal
///   identity, weak operational.
/// - **Executive** — board members, outside counsel, executives. Strong
///   oversight, deliberate.
/// - **Operator** — MSPs, other admins, signing services. Strong
///   operational, fast access.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum TrusteeRoleGroup {
    /// Family, partners, close friends.
    Personal,
    /// Board members, outside counsel, executives.
    Executive,
    /// MSPs, other admins, signing services.
    Operator,
}

/// Lifecycle status of a recovery method enrollment.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MethodStatus {
    /// Active and counted toward the bundle's threshold.
    Active,
    /// Declared but not yet enrolled; doesn't count toward effective
    /// threshold until promoted to `Active`.
    Provisional {
        /// When this method is expected to complete enrollment.
        /// Stored as a Unix timestamp in seconds.
        expected_by_unix: u64,
    },
    /// Pending removal during a cooldown window; still counts as active
    /// until the cooldown elapses or the removal is cancelled.
    PendingRemoval,
    /// Revoked; no longer participates in recovery; kept for audit.
    Revoked,
}

/// Identifier for the share-combining scheme used by a bundle.
///
/// Versioned so future bundles can adopt new schemes (e.g., threshold
/// signatures) without breaking existing tooling.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ShareScheme {
    /// Shamir Secret Sharing over GF(2^8), bytewise on the 32-byte KEK.
    /// AEAD on the bundle plaintext is the caller's responsibility.
    ShamirAeadV1,
}

// ---------------------------------------------------------------------------
// Effective threshold: counting methods by status
// ---------------------------------------------------------------------------

/// A method's role plus its lifecycle status.
///
/// Used by [`RecoveryPolicy::evaluate_with_enrollments`] and
/// [`effective_threshold`] to compute what's actually recoverable
/// given that some enrollments may be `Provisional` or `Revoked`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MethodEnrollment {
    /// Organizational role of this method.
    pub role: MethodRole,
    /// Current lifecycle status.
    pub status: MethodStatus,
}

impl MethodEnrollment {
    /// True if the enrollment counts toward the effective threshold.
    ///
    /// `Active` and `PendingRemoval` methods are share-bearing and
    /// count. `Provisional` methods are declared but not yet
    /// contributing share material. `Revoked` methods are excluded
    /// (kept for audit only).
    pub fn counts_toward_threshold(&self) -> bool {
        matches!(
            self.status,
            MethodStatus::Active | MethodStatus::PendingRemoval
        )
    }
}

/// The effective threshold for a method set.
///
/// `effective_k` is the smallest number of share-bearing methods
/// required for recovery, clamped to never exceed `effective_n`.
/// `effective_n` is the count of methods currently contributing
/// share material (`Active` + `PendingRemoval`).
///
/// `is_recoverable()` is the headline check the bootstrap and
/// recovery ceremonies gate on: an install with `effective_n == 0`
/// has no recoverable methods at all.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectiveThreshold {
    /// Number of shares required, after clamping to `effective_n`.
    pub effective_k: u32,
    /// Number of shares currently contributing material.
    pub effective_n: u32,
    /// The originally declared K (preserved for diagnostics — UIs
    /// surface "X of Y configured, but only Z are active right now").
    pub declared_k: u32,
    /// The originally declared N.
    pub declared_n: u32,
    /// Number of `Provisional` enrollments not currently counted.
    pub provisional_count: u32,
    /// Number of `Revoked` enrollments not currently counted.
    pub revoked_count: u32,
}

impl EffectiveThreshold {
    /// True when at least one share-bearing method is present and the
    /// effective threshold can be satisfied by the share-bearing set.
    pub fn is_recoverable(&self) -> bool {
        self.effective_n > 0 && self.effective_k > 0
    }

    /// True if any reductions were applied versus the declared values.
    pub fn was_reduced(&self) -> bool {
        self.effective_n != self.declared_n || self.effective_k != self.declared_k
    }
}

/// Compute the effective threshold for the given declared policy and
/// the actual enrollment set.
///
/// The reduction rule is:
///
/// - `effective_n` = count of `Active` + `PendingRemoval` enrollments
/// - `effective_k` = `min(declared_k, effective_n)`
/// - `Provisional` and `Revoked` enrollments are tracked in the
///   `*_count` fields for UI diagnostics but don't count toward
///   either k or n.
///
/// When `effective_n` is 0, the install has no recoverable methods —
/// the bootstrap or rotation that produced this state was incoherent
/// and recovery cannot proceed. [`EffectiveThreshold::is_recoverable`]
/// surfaces this.
pub fn effective_threshold(
    declared: Threshold,
    enrollments: &[MethodEnrollment],
) -> EffectiveThreshold {
    let mut effective_n: u32 = 0;
    let mut provisional_count: u32 = 0;
    let mut revoked_count: u32 = 0;
    for e in enrollments {
        match e.status {
            MethodStatus::Active | MethodStatus::PendingRemoval => effective_n += 1,
            MethodStatus::Provisional { .. } => provisional_count += 1,
            MethodStatus::Revoked => revoked_count += 1,
        }
    }
    let effective_k = declared.k.min(effective_n);
    EffectiveThreshold {
        effective_k,
        effective_n,
        declared_k: declared.k,
        declared_n: declared.n,
        provisional_count,
        revoked_count,
    }
}

impl RecoveryPolicy {
    /// Evaluate this policy over a status-aware enrollment set.
    ///
    /// Like [`Self::evaluate`] but consumes [`MethodEnrollment`]s so
    /// `Provisional` and `Revoked` methods are ignored at the
    /// threshold step and constraints. Returns the [`EffectiveThreshold`]
    /// alongside the success / failure so callers can show "you are
    /// using K of effective_N (declared K of N)".
    pub fn evaluate_with_enrollments(
        &self,
        enrollments: &[MethodEnrollment],
    ) -> (Result<(), BundleError>, EffectiveThreshold) {
        let effective = effective_threshold(self.threshold, enrollments);
        if !effective.is_recoverable() {
            return (
                Err(BundleError::ConstraintFailed(format!(
                    "no share-bearing methods: effective_n=0 (declared {}/{}, \
                     provisional={}, revoked={})",
                    self.threshold.k,
                    self.threshold.n,
                    effective.provisional_count,
                    effective.revoked_count
                ))),
                effective,
            );
        }
        let counted: Vec<MethodRole> = enrollments
            .iter()
            .filter(|e| e.counts_toward_threshold())
            .map(|e| e.role.clone())
            .collect();
        if (counted.len() as u32) < effective.effective_k {
            return (
                Err(BundleError::ConstraintFailed(format!(
                    "threshold not met: need {} share-bearing methods, got {}",
                    effective.effective_k,
                    counted.len()
                ))),
                effective,
            );
        }
        for c in &self.constraints {
            if let Err(e) = c.evaluate(&counted) {
                return (Err(e), effective);
            }
        }
        (Ok(()), effective)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn admin(id_byte: u8) -> MethodRole {
        // Build a deterministic AdminId from a single byte for test
        // distinguishability; the rest of the UUID is zero-padded.
        let mut bytes = [0u8; 16];
        bytes[0] = id_byte;
        MethodRole::AdminFactor {
            admin_id: AdminId(uuid::Uuid::from_bytes(bytes)),
        }
    }

    fn trustee(id_byte: u8, group: TrusteeRoleGroup) -> MethodRole {
        let mut bytes = [0u8; 16];
        bytes[0] = id_byte;
        MethodRole::ExternalTrustee {
            trustee_id: TrusteeId(uuid::Uuid::from_bytes(bytes)),
            role_group: group,
        }
    }

    fn org() -> MethodRole {
        MethodRole::OrgInfrastructure
    }

    fn sys() -> MethodRole {
        MethodRole::SystemFactor
    }

    // -- threshold --------------------------------------------------

    #[test]
    fn threshold_only_passes_at_or_above_k() {
        let p = RecoveryPolicy::threshold_only(Threshold::new(2, 3).unwrap());
        assert!(p.evaluate(&[admin(1), admin(2)]).is_ok());
        assert!(p.evaluate(&[admin(1), admin(2), admin(3)]).is_ok());
    }

    #[test]
    fn threshold_only_refuses_below_k() {
        let p = RecoveryPolicy::threshold_only(Threshold::new(2, 3).unwrap());
        let result = p.evaluate(&[admin(1)]);
        assert!(matches!(result, Err(BundleError::ConstraintFailed(_))));
    }

    #[test]
    fn threshold_zero_methods_refused() {
        let p = RecoveryPolicy::threshold_only(Threshold::new(2, 3).unwrap());
        let result = p.evaluate(&[]);
        assert!(matches!(result, Err(BundleError::ConstraintFailed(_))));
    }

    #[test]
    fn threshold_unanimous_requires_all_enrolled() {
        let p = RecoveryPolicy::threshold_only(Threshold::new(3, 3).unwrap());
        assert!(p.evaluate(&[admin(1), admin(2), admin(3)]).is_ok());
        assert!(p.evaluate(&[admin(1), admin(2)]).is_err());
    }

    // -- AtLeastOneOfRole ------------------------------------------

    #[test]
    fn at_least_one_external_trustee_passes_when_present() {
        let p = RecoveryPolicy {
            threshold: Threshold::new(2, 3).unwrap(),
            constraints: vec![RecoveryConstraint::AtLeastOneOfRole {
                role: MethodRoleKind::ExternalTrustee,
            }],
        };
        assert!(p
            .evaluate(&[admin(1), trustee(1, TrusteeRoleGroup::Personal)])
            .is_ok());
    }

    #[test]
    fn at_least_one_external_trustee_refused_when_absent() {
        let p = RecoveryPolicy {
            threshold: Threshold::new(2, 3).unwrap(),
            constraints: vec![RecoveryConstraint::AtLeastOneOfRole {
                role: MethodRoleKind::ExternalTrustee,
            }],
        };
        let result = p.evaluate(&[admin(1), admin(2)]);
        assert!(matches!(result, Err(BundleError::ConstraintFailed(_))));
        assert!(
            format!("{:?}", result.unwrap_err()).contains("AtLeastOneOfRole"),
            "error should name the failed constraint"
        );
    }

    // -- AtMostOneOfRole -------------------------------------------

    #[test]
    fn at_most_one_admin_factor_allows_one() {
        let p = RecoveryPolicy {
            threshold: Threshold::new(2, 3).unwrap(),
            constraints: vec![RecoveryConstraint::AtMostOneOfRole {
                role: MethodRoleKind::AdminFactor,
                max: 1,
            }],
        };
        assert!(p
            .evaluate(&[admin(1), trustee(1, TrusteeRoleGroup::Personal)])
            .is_ok());
    }

    #[test]
    fn at_most_one_admin_factor_refuses_two() {
        let p = RecoveryPolicy {
            threshold: Threshold::new(2, 3).unwrap(),
            constraints: vec![RecoveryConstraint::AtMostOneOfRole {
                role: MethodRoleKind::AdminFactor,
                max: 1,
            }],
        };
        let result = p.evaluate(&[admin(1), admin(2)]);
        assert!(matches!(result, Err(BundleError::ConstraintFailed(_))));
    }

    #[test]
    fn at_most_zero_means_role_forbidden() {
        let p = RecoveryPolicy {
            threshold: Threshold::new(2, 3).unwrap(),
            constraints: vec![RecoveryConstraint::AtMostOneOfRole {
                role: MethodRoleKind::SystemFactor,
                max: 0,
            }],
        };
        assert!(p.evaluate(&[admin(1), admin(2)]).is_ok());
        assert!(p.evaluate(&[admin(1), sys()]).is_err());
    }

    // -- DistinctHolders -------------------------------------------

    #[test]
    fn distinct_holders_passes_with_unique_admins() {
        let p = RecoveryPolicy {
            threshold: Threshold::new(2, 3).unwrap(),
            constraints: vec![RecoveryConstraint::DistinctHolders { minimum: 2 }],
        };
        assert!(p.evaluate(&[admin(1), admin(2)]).is_ok());
    }

    #[test]
    fn distinct_holders_refused_when_same_admin_used_twice() {
        let p = RecoveryPolicy {
            threshold: Threshold::new(2, 3).unwrap(),
            constraints: vec![RecoveryConstraint::DistinctHolders { minimum: 2 }],
        };
        // Same admin_id submitted twice (e.g., one admin's two methods).
        let result = p.evaluate(&[admin(1), admin(1)]);
        assert!(matches!(result, Err(BundleError::ConstraintFailed(_))));
    }

    #[test]
    fn distinct_holders_collapses_repeated_org_infrastructure() {
        // OrgInfrastructure is a unit variant; repeated entries hash
        // to the same key and count as a single distinct holder.
        // This is intentional: org infrastructure is "one entity"
        // for the holder-distinctness rule.
        let p = RecoveryPolicy {
            threshold: Threshold::new(2, 3).unwrap(),
            constraints: vec![RecoveryConstraint::DistinctHolders { minimum: 2 }],
        };
        let result = p.evaluate(&[org(), org()]);
        assert!(
            matches!(result, Err(BundleError::ConstraintFailed(_))),
            "two OrgInfrastructure entries collapse to one distinct holder"
        );
    }

    #[test]
    fn distinct_holders_passes_with_admin_plus_org_plus_trustee() {
        let p = RecoveryPolicy {
            threshold: Threshold::new(3, 5).unwrap(),
            constraints: vec![RecoveryConstraint::DistinctHolders { minimum: 3 }],
        };
        assert!(p
            .evaluate(&[admin(1), org(), trustee(1, TrusteeRoleGroup::Personal)])
            .is_ok());
    }

    // -- DistinctTrusteeRoleGroups ---------------------------------

    #[test]
    fn distinct_trustee_role_groups_passes_with_two_groups() {
        let p = RecoveryPolicy {
            threshold: Threshold::new(2, 4).unwrap(),
            constraints: vec![RecoveryConstraint::DistinctTrusteeRoleGroups { minimum: 2 }],
        };
        assert!(p
            .evaluate(&[
                trustee(1, TrusteeRoleGroup::Personal),
                trustee(2, TrusteeRoleGroup::Executive),
            ])
            .is_ok());
    }

    #[test]
    fn distinct_trustee_role_groups_refused_when_all_same_group() {
        let p = RecoveryPolicy {
            threshold: Threshold::new(2, 4).unwrap(),
            constraints: vec![RecoveryConstraint::DistinctTrusteeRoleGroups { minimum: 2 }],
        };
        let result = p.evaluate(&[
            trustee(1, TrusteeRoleGroup::Personal),
            trustee(2, TrusteeRoleGroup::Personal),
        ]);
        assert!(
            matches!(result, Err(BundleError::ConstraintFailed(_))),
            "two trustees from the same group should fail DistinctTrusteeRoleGroups(2)"
        );
    }

    #[test]
    fn distinct_trustee_role_groups_ignores_non_trustee_methods() {
        // The constraint counts ONLY trustees. An admin + org + one
        // trustee yields 1 group, not 3.
        let p = RecoveryPolicy {
            threshold: Threshold::new(3, 4).unwrap(),
            constraints: vec![RecoveryConstraint::DistinctTrusteeRoleGroups { minimum: 2 }],
        };
        let result = p.evaluate(&[admin(1), org(), trustee(1, TrusteeRoleGroup::Personal)]);
        assert!(matches!(result, Err(BundleError::ConstraintFailed(_))));
    }

    // -- Compound policies -----------------------------------------

    #[test]
    fn compound_policy_requires_all_constraints() {
        // 2-of-3 + at-least-one-trustee + distinct-holders(2)
        let p = RecoveryPolicy {
            threshold: Threshold::new(2, 3).unwrap(),
            constraints: vec![
                RecoveryConstraint::AtLeastOneOfRole {
                    role: MethodRoleKind::ExternalTrustee,
                },
                RecoveryConstraint::DistinctHolders { minimum: 2 },
            ],
        };
        // Two admins satisfy threshold + holders, but no trustee.
        assert!(p.evaluate(&[admin(1), admin(2)]).is_err());
        // Two trustees from same identity satisfy threshold + trustee
        // role, but only one distinct holder.
        assert!(p
            .evaluate(&[
                trustee(1, TrusteeRoleGroup::Personal),
                trustee(1, TrusteeRoleGroup::Personal),
            ])
            .is_err());
        // Admin + trustee — all three constraints met.
        assert!(p
            .evaluate(&[admin(1), trustee(1, TrusteeRoleGroup::Personal)])
            .is_ok());
    }

    #[test]
    fn enterprise_pattern_requires_trustee_and_distinct_groups() {
        // Real-world enterprise pattern: 3-of-5, at least one external
        // trustee, trustee shares span at least 2 distinct role groups.
        let p = RecoveryPolicy {
            threshold: Threshold::new(3, 5).unwrap(),
            constraints: vec![
                RecoveryConstraint::AtLeastOneOfRole {
                    role: MethodRoleKind::ExternalTrustee,
                },
                RecoveryConstraint::DistinctTrusteeRoleGroups { minimum: 2 },
            ],
        };
        // 2 admins + 1 trustee: AtLeastOne ✓ but DistinctGroups(2) fails
        // (only 1 group represented).
        let r = p.evaluate(&[admin(1), admin(2), trustee(1, TrusteeRoleGroup::Personal)]);
        assert!(matches!(r, Err(BundleError::ConstraintFailed(_))));
        // Same K, but trustees span Personal + Executive groups.
        let r = p.evaluate(&[
            admin(1),
            trustee(1, TrusteeRoleGroup::Personal),
            trustee(2, TrusteeRoleGroup::Executive),
        ]);
        assert!(r.is_ok());
    }

    #[test]
    fn evaluation_short_circuits_on_first_failure() {
        // Threshold fails first; constraints never evaluated.
        let p = RecoveryPolicy {
            threshold: Threshold::new(3, 5).unwrap(),
            constraints: vec![RecoveryConstraint::AtLeastOneOfRole {
                role: MethodRoleKind::ExternalTrustee,
            }],
        };
        let r = p.evaluate(&[admin(1)]);
        let msg = format!("{:?}", r.unwrap_err());
        assert!(
            msg.contains("threshold"),
            "expected threshold failure message; got: {msg}"
        );
    }

    // -- Constraint serialization (already covered upstream, but
    //    pinning the wire-format here to catch silent breaks) ------

    #[test]
    fn constraint_serializes_with_kind_tag() {
        let c = RecoveryConstraint::AtLeastOneOfRole {
            role: MethodRoleKind::ExternalTrustee,
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"kind\":\"AtLeastOneOfRole\""));
        assert!(json.contains("\"role\":\"ExternalTrustee\""));
    }

    // -- Provisional / effective threshold --------------------------

    fn enroll(role: MethodRole, status: MethodStatus) -> MethodEnrollment {
        MethodEnrollment { role, status }
    }

    #[test]
    fn effective_threshold_all_active_matches_declared() {
        let declared = Threshold::new(3, 5).unwrap();
        let enrollments = vec![
            enroll(admin(1), MethodStatus::Active),
            enroll(admin(2), MethodStatus::Active),
            enroll(admin(3), MethodStatus::Active),
            enroll(org(), MethodStatus::Active),
            enroll(trustee(1, TrusteeRoleGroup::Personal), MethodStatus::Active),
        ];
        let eff = effective_threshold(declared, &enrollments);
        assert_eq!(eff.effective_k, 3);
        assert_eq!(eff.effective_n, 5);
        assert_eq!(eff.declared_k, 3);
        assert_eq!(eff.declared_n, 5);
        assert_eq!(eff.provisional_count, 0);
        assert_eq!(eff.revoked_count, 0);
        assert!(eff.is_recoverable());
        assert!(!eff.was_reduced());
    }

    #[test]
    fn effective_threshold_provisional_methods_excluded() {
        // Bootstrap commits with 2 Active + 3 Provisional methods at
        // declared 3-of-5. Effective is 2-of-2 (clamped).
        let declared = Threshold::new(3, 5).unwrap();
        let enrollments = vec![
            enroll(admin(1), MethodStatus::Active),
            enroll(admin(2), MethodStatus::Active),
            enroll(
                admin(3),
                MethodStatus::Provisional {
                    expected_by_unix: 1_700_000_000,
                },
            ),
            enroll(
                org(),
                MethodStatus::Provisional {
                    expected_by_unix: 1_700_000_000,
                },
            ),
            enroll(
                trustee(1, TrusteeRoleGroup::Personal),
                MethodStatus::Provisional {
                    expected_by_unix: 1_700_000_000,
                },
            ),
        ];
        let eff = effective_threshold(declared, &enrollments);
        assert_eq!(eff.effective_k, 2, "K clamped from 3 down to 2");
        assert_eq!(eff.effective_n, 2);
        assert_eq!(eff.provisional_count, 3);
        assert!(eff.is_recoverable());
        assert!(eff.was_reduced());
    }

    #[test]
    fn effective_threshold_revoked_methods_excluded() {
        let declared = Threshold::new(2, 3).unwrap();
        let enrollments = vec![
            enroll(admin(1), MethodStatus::Active),
            enroll(admin(2), MethodStatus::Active),
            enroll(admin(3), MethodStatus::Revoked),
        ];
        let eff = effective_threshold(declared, &enrollments);
        assert_eq!(eff.effective_k, 2);
        assert_eq!(eff.effective_n, 2);
        assert_eq!(eff.revoked_count, 1);
    }

    #[test]
    fn effective_threshold_pending_removal_still_counts() {
        // PendingRemoval methods are still share-bearing during cooldown.
        let declared = Threshold::new(2, 3).unwrap();
        let enrollments = vec![
            enroll(admin(1), MethodStatus::Active),
            enroll(admin(2), MethodStatus::PendingRemoval),
            enroll(admin(3), MethodStatus::Active),
        ];
        let eff = effective_threshold(declared, &enrollments);
        assert_eq!(eff.effective_n, 3);
    }

    #[test]
    fn effective_threshold_zero_active_is_unrecoverable() {
        let declared = Threshold::new(2, 3).unwrap();
        let enrollments = vec![
            enroll(
                admin(1),
                MethodStatus::Provisional {
                    expected_by_unix: 0,
                },
            ),
            enroll(admin(2), MethodStatus::Revoked),
        ];
        let eff = effective_threshold(declared, &enrollments);
        assert_eq!(eff.effective_n, 0);
        assert_eq!(eff.effective_k, 0);
        assert!(!eff.is_recoverable());
    }

    #[test]
    fn evaluate_with_enrollments_passes_at_effective_k() {
        let p = RecoveryPolicy {
            threshold: Threshold::new(3, 5).unwrap(),
            constraints: vec![],
        };
        // 2 Active + 3 Provisional. Effective 2-of-2; K=2 share-
        // bearing methods provided.
        let enrollments = vec![
            enroll(admin(1), MethodStatus::Active),
            enroll(admin(2), MethodStatus::Active),
            enroll(
                admin(3),
                MethodStatus::Provisional {
                    expected_by_unix: 0,
                },
            ),
            enroll(
                org(),
                MethodStatus::Provisional {
                    expected_by_unix: 0,
                },
            ),
            enroll(
                trustee(1, TrusteeRoleGroup::Personal),
                MethodStatus::Provisional {
                    expected_by_unix: 0,
                },
            ),
        ];
        let (result, eff) = p.evaluate_with_enrollments(&enrollments);
        assert!(result.is_ok());
        assert_eq!(eff.effective_k, 2);
    }

    #[test]
    fn evaluate_with_enrollments_refused_when_no_active_methods() {
        let p = RecoveryPolicy {
            threshold: Threshold::new(2, 3).unwrap(),
            constraints: vec![],
        };
        let enrollments = vec![
            enroll(
                admin(1),
                MethodStatus::Provisional {
                    expected_by_unix: 0,
                },
            ),
            enroll(admin(2), MethodStatus::Revoked),
        ];
        let (result, eff) = p.evaluate_with_enrollments(&enrollments);
        assert!(matches!(result, Err(BundleError::ConstraintFailed(_))));
        assert!(!eff.is_recoverable());
    }

    #[test]
    fn evaluate_with_enrollments_constraints_apply_only_to_counted() {
        // AtLeastOneOfRole(ExternalTrustee) — the only trustee is
        // Provisional. Constraint should fail because share-bearing
        // set has no trustee.
        let p = RecoveryPolicy {
            threshold: Threshold::new(2, 3).unwrap(),
            constraints: vec![RecoveryConstraint::AtLeastOneOfRole {
                role: MethodRoleKind::ExternalTrustee,
            }],
        };
        let enrollments = vec![
            enroll(admin(1), MethodStatus::Active),
            enroll(admin(2), MethodStatus::Active),
            enroll(
                trustee(1, TrusteeRoleGroup::Personal),
                MethodStatus::Provisional {
                    expected_by_unix: 0,
                },
            ),
        ];
        let (result, _) = p.evaluate_with_enrollments(&enrollments);
        assert!(
            matches!(result, Err(BundleError::ConstraintFailed(_))),
            "Provisional trustee must not satisfy AtLeastOneOfRole"
        );
    }

    #[test]
    fn evaluate_with_enrollments_diagnostic_lists_provisional_and_revoked() {
        let p = RecoveryPolicy::threshold_only(Threshold::new(2, 3).unwrap());
        let enrollments = vec![
            enroll(
                admin(1),
                MethodStatus::Provisional {
                    expected_by_unix: 0,
                },
            ),
            enroll(admin(2), MethodStatus::Revoked),
        ];
        let (result, _) = p.evaluate_with_enrollments(&enrollments);
        let msg = format!("{:?}", result.unwrap_err());
        assert!(msg.contains("provisional=1"));
        assert!(msg.contains("revoked=1"));
    }

    #[test]
    fn method_enrollment_counts_toward_threshold_helper() {
        let active = enroll(admin(1), MethodStatus::Active);
        let pending = enroll(admin(2), MethodStatus::PendingRemoval);
        let provisional = enroll(
            admin(3),
            MethodStatus::Provisional {
                expected_by_unix: 0,
            },
        );
        let revoked = enroll(admin(4), MethodStatus::Revoked);
        assert!(active.counts_toward_threshold());
        assert!(pending.counts_toward_threshold());
        assert!(!provisional.counts_toward_threshold());
        assert!(!revoked.counts_toward_threshold());
    }

    #[test]
    fn policy_round_trips_through_serde() {
        let p = RecoveryPolicy {
            threshold: Threshold::new(2, 5).unwrap(),
            constraints: vec![
                RecoveryConstraint::AtLeastOneOfRole {
                    role: MethodRoleKind::ExternalTrustee,
                },
                RecoveryConstraint::AtMostOneOfRole {
                    role: MethodRoleKind::AdminFactor,
                    max: 1,
                },
                RecoveryConstraint::DistinctHolders { minimum: 3 },
                RecoveryConstraint::DistinctTrusteeRoleGroups { minimum: 2 },
            ],
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: RecoveryPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }
}
