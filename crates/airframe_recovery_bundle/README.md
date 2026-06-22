# airframe_recovery_bundle

Recovery bundle format and K-of-N share-combining for the Airframe ceremony framework.

**Format-only.** This crate does not perform AEAD encryption of bundle plaintext — the caller (typically Nanokey) handles that with whatever cipher they trust. This crate provides:

- The data structures: `RecoveryBundle`, `BundlePayload`, `RecoveryPolicy`, `RecoveryConstraint`, `MethodRole`, `MethodRoleKind`, `TrusteeRoleGroup`, `MethodStatus`, `ShareScheme`
- Shamir-on-KEK split and combine via `split_kek` and `combine_shares`
- Constraint evaluation against a list of methods used in a recovery attempt

## Threat model fit

The recovery bundle is the install's lifeline. It contains the install's identity (`InstallId`), the master signing key material, and the Cache Key (`CK`). The bundle ciphertext is opaque to this crate — encryption and decryption happen in Nanokey under a recovery KEK that's then split via Shamir.

Sensitive types (`Kek`, `BundlePayload`) implement `Zeroize` and `ZeroizeOnDrop` so secret bytes are wiped from memory when values go out of scope. `Debug` implementations never print secret bytes.

## Usage

```rust
use airframe_id::Threshold;
use airframe_recovery_bundle::{
    Kek, RecoveryBundle, RecoveryPolicy, ShareScheme, split_kek, combine_shares,
};

// At bundle-mint time, after the caller produces a 32-byte KEK and AEAD-encrypts the bundle plaintext:
let kek = Kek([0u8; 32]); // produced by Nanokey, not this crate
let bundle = RecoveryBundle::new(
    /* bundle_version */ 1,
    /* prior_bundle_id */ None,
    RecoveryPolicy::threshold_only(Threshold::new(2, 3).unwrap()),
    ShareScheme::ShamirAeadV1,
    /* ciphertext */ vec![/* AEAD output here */],
);

// Split the KEK into shares, one per enrolled method:
let shares = split_kek(&kek, /* k */ 2, /* n */ 3, bundle.bundle_version).unwrap();

// At recovery time, combine K of the N shares:
let recovered = combine_shares(2, &shares[..2]).unwrap();
assert_eq!(recovered.as_bytes(), kek.as_bytes());
```

## Constraint evaluation

```rust
use airframe_id::AdminId;
use airframe_recovery_bundle::{
    MethodRole, MethodRoleKind, RecoveryConstraint, RecoveryPolicy, TrusteeRoleGroup,
};

let policy = RecoveryPolicy {
    threshold: airframe_id::Threshold::new(2, 5).unwrap(),
    constraints: vec![
        RecoveryConstraint::AtLeastOneOfRole {
            role: MethodRoleKind::ExternalTrustee,
        },
    ],
};

let methods_used = vec![
    MethodRole::AdminFactor { admin_id: AdminId::new() },
    MethodRole::ExternalTrustee {
        trustee_id: airframe_id::TrusteeId::new(),
        role_group: TrusteeRoleGroup::Executive,
    },
];

policy.evaluate(&methods_used).unwrap();
```

## Specification

The authoritative specification lives in `docs/arch-recovery-system.md` and `docs/ref-ceremony-types.md` in the airspace repo.

## Dependencies

- `airframe_id` — shared identifier types
- `sharks` — Shamir Secret Sharing implementation (vetted Rust crate)
- `serde` + `serde_derive` — serialization
- `zeroize` — secure memory wiping for sensitive types
- `thiserror` — error type derivation

## Testing

```bash
cargo test -p airframe_recovery_bundle
```
