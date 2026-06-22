//! Recovery bundle format and K-of-N share-combining for the Airframe
//! ceremony framework.
//!
//! This crate is **format only**. It does not perform AEAD encryption
//! of bundle plaintext — callers (typically Nanokey) handle that with
//! whatever cipher they trust. This crate provides:
//!
//! - The data structures: [`RecoveryBundle`], [`BundlePayload`],
//!   [`RecoveryPolicy`], [`RecoveryConstraint`], [`MethodRole`],
//!   [`MethodRoleKind`], [`TrusteeRoleGroup`], [`ShareScheme`].
//! - Shamir-on-KEK split and combine via [`split_kek`] and
//!   [`combine_shares`].
//! - Constraint evaluation against a list of methods used in a recovery
//!   attempt.
//!
//! See `docs/arch-recovery-system.md` and `docs/ref-ceremony-types.md`
//! in the airspace repo for the authoritative specification.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod bundle;
mod error;
mod policy;
mod rotation;
mod share;

pub use bundle::{BundlePayload, Kek, RecoveryBundle};
pub use error::{BundleError, ShareError};
pub use policy::{
    effective_threshold, EffectiveThreshold, MethodEnrollment, MethodRole, MethodRoleKind,
    MethodStatus, RecoveryConstraint, RecoveryPolicy, ShareScheme, TrusteeRoleGroup,
};
pub use rotation::{
    change_threshold_required_quorum, rotate_add_method, rotate_change_threshold,
    rotate_credential, rotate_remove_method, RotatedBundle,
};
pub use share::{
    combine_shares, combine_shares_checked, commit_shares, share_commitment, split_kek, Share,
    ShareCommitment,
};
