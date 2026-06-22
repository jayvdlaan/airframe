# airframe_id

Shared identifier types for the Airframe ceremony framework.

These types are deliberately minimal — newtype wrappers around `Uuid` or fixed-size byte arrays — so they can be depended on by every layer (framework, recovery, cross-service orchestration) without pulling in heavier abstractions.

## Types

| Type | Purpose |
|---|---|
| `InstallId` | 128-bit shared identity between Nanokey and Nanopass; load-bearing for foot-gun guards |
| `CeremonyId` | Identifier for a ceremony instance |
| `MethodId` | Identifier for an enrolled recovery method |
| `BundleId` | Identifier for a recovery bundle revision |
| `AdminId` | Identifier for a named administrator |
| `TrusteeId` | Identifier for an external recovery trustee |
| `Threshold` | K-of-N threshold over enrolled methods or trustees |

## Usage

```rust
use airframe_id::{InstallId, CeremonyId, Threshold};

// At first bootstrap only — preserved across recovery thereafter
let install_id = InstallId::new();

let ceremony = CeremonyId::new();

let policy = Threshold::new(2, 3).expect("valid k-of-n");
assert!(policy.satisfiable_by(2));
```

## Specification

The authoritative specification for these types lives in `docs/ref-ceremony-types.md` in the airspace repo. Bug reports about type semantics should reference that spec.

## Testing

```bash
cargo test -p airframe_id
```
