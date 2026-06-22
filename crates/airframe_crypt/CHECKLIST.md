# airframe_crypt — Argon2 Integration Checklist

Legend: [ ] = TODO, [*] = In progress, [x] = Done

Goal: Add Argon2 (prefer Argon2id) KDF support to airframe_crypt with clean API, feature-gated dependency, provider integration, and tests/docs.

Phase A — API surface and identifiers
- [x] AlgorithmId: add `Argon2id` variant with `as_str()`/`from_str()` mappings.
- [x] KDF API: extend `CipherSuite` trait with `argon2(password, salt, params, out_len)`.
- [x] Provider surface: extend `KdfProvider` with `argon2(...)` and wire through `ProviderCipherSuite`.

Phase B — Implementation
- [x] Add `Argon2Params` (variant, m_cost_kib, t_cost, p_cost, version) and `Argon2Variant` enums with sensible `Default` (Argon2id, 64 MiB, t=3, p=1, v=0x13).
- [x] Implement RustCrypto-backed derivation in `kdf.rs` behind `argon2` feature using `argon2::{Argon2, Algorithm, Params, Version}`.
- [x] Validate parameters (min salt length, sane bounds for memory/time/parallelism) and return clear errors.
- [x] Provide concrete provider (e.g., `RustCryptoKdfProvider`) implementing both PBKDF2 and Argon2.
- [x] Wire `SoftwareCipherSuite::argon2` to call the RustCrypto implementation.

Phase C — Cargo features and deps
- [x] Cargo.toml: add optional dependency `argon2 = { version = "0.5", default-features = false, optional = true, features = ["std", "password-hash"] }`.
- [x] Features: add `argon2` feature; decide whether to include it in `default` (opt-in by default recommended).

Phase D — Tests and examples
- [x] Unit tests: Argon2id derive basic vector (length, non-zero) behind `#[cfg(all(test, feature = "argon2"))]`.
- [x] Suite tests: `SoftwareCipherSuite::argon2` happy path; `ProviderCipherSuite::argonon2` unsupported path when provider missing.
- [ ] Optional: compare against RFC 9106 vectors (ensure exact params match) or self-generated reference.
- [x] Example: `examples/kdf_argon2.rs` showing interactive parameters and derivation.

Phase E — Documentation
- [x] README: document Argon2 support, feature flag, recommended parameters, and security notes (prefer Argon2id).
- [x] API docs: document `Argon2Params` fields and defaults.

Phase F — Integration guidance
- [x] Note for Nanokey: enable feature `airframe_crypt = { features = ["argon2"] }`; add `Argon2id` option in factor policy; derive with per-factor salts via `SecretCache`.

Phase G — Security and bounds
- [x] Enforce minimum salt length (>= 16 bytes) in Argon2 API or document requirement.
- [x] Cap `out_len` to a reasonable maximum (e.g., 64 bytes) or document intended usage.
- [x] Prefer Argon2id for user-supplied secrets; state rationale in docs.

Completion criteria
- [x] All API changes compile with feature on/off.
- [x] Tests pass with `--features argon2` and without.
- [x] README/docs updated; example builds and runs.
