# AGENTS.md — Contributor playbook for AI agents

**If you are an LLM or coding agent preparing a contribution to Airframe, this
file is your contract. Read it in full before editing anything.** A human asked
you to open a pull request here; your job is to produce a change that a
maintainer can merge *without rework*.

Airframe was largely built with AI and is meant to be worked on with AI.
AI-assisted PRs are **welcome and expected** — but the bar is high precisely
*because* an agent can move fast. Speed without verification is the failure mode
we reject. Everything below exists to keep that from happening.

---

## TL;DR — the gates (all must be green, no exceptions)

**This project has no CI, by design** (a deliberate supply-chain–risk decision:
no third-party runners, marketplace actions, or stored tokens). The gate is run
**manually**, and a contribution is only as trustworthy as the output you paste.
Run the whole gate with one recipe from the repository root and paste its real
output into your PR:

```bash
just release-check    # fmt-check + clippy(-D warnings,--all-targets) + test + build
                      # + every advertised feature combination + docs
```

The maintainer re-runs `just release-check` locally before merging — so a green
paste that doesn't actually reproduce is worse than an honest red one. The
individual steps (`just fmt-check`, `just clippy`, `just test`, `just build`)
exist too if you want to iterate on one.

**Every feature combination you touched must compile.** If you changed a crate
that gates behaviour behind features (`module`, `driver`, `http`, etc.), make
sure `release-check` covers the combination, or build it explicitly — a crate
that builds with default features can still fail under another.

If any gate is red, the change is not done. Do not suppress a lint, delete a
failing test, or weaken an assertion to make a gate pass.

---

## 1. Before you change anything

Read, in this order:

1. **`docs/arch-design.md`** — the L0–L7 layer model and the dependency rules.
2. **`../spacetime/docs/ref-project-conventions.md`** — the authoritative
   reference for crate naming, file layout, `Cargo.toml` conventions,
   versioning, documentation naming, and the Release-Readiness checklist. This
   is not optional reading; conventions here are enforced.
3. **The `README.md` of the specific crate you are modifying** — every crate has
   one describing its purpose, module compatibility, and status.

If you cannot articulate which layer your change lives in and why its
dependencies only point *downward*, stop and work that out first.

---

## 2. Non-negotiable rules

### Layering
Dependencies point **downward only**. A crate at layer N may depend on layers
≤ N, never above. `airframe_core` (L0) depends on nothing in this workspace.
Adding an upward or lateral dependency to make something convenient is a
rejection, not a shortcut. If two crates need to talk without an ordering
dependency, wire them at runtime through the `ServiceRegistry`, not through a
module edge.

### Security model — read this twice
This is a security-focused framework. The crypto boundary is sacred:

- **All cryptography and cryptographic randomness for security decisions goes
  through the boundary** (`airframe_crypt` providers, and at the application
  tier, Nanokey). Do **not** hand-roll crypto, reach for `rand` for a security
  decision, or call an OpenSSL/RustCrypto primitive directly outside a provider
  implementation.
- **The crypto boundary must not leak its backend.** Public trait surfaces take
  backend-agnostic types (e.g. PEM-wrapped keys), never `openssl::PKey` or a
  concrete library type. If your change makes a backend type appear in a public
  signature, it is wrong.
- **Never log or persist plaintext secrets.** Redact connection strings, keys,
  tokens. Secret-bearing structs should zeroize on drop.
- **Fail closed.** On a security-relevant error, deny — never default-allow.
  Reject malformed input (path traversal, oversized decompression, low-order
  curve points) rather than processing it.

If you are unsure whether something is "a security decision," treat it as one.

### Conventions
Follow `ref-project-conventions.md` exactly: `airframe_*` snake_case crate
names; one concern per file (no god-files); `version`/`edition`/`license`/
`repository` inherited via `*.workspace = true`; features additive and
consistent; every new crate gets a `README.md`. Match the surrounding code's
style, naming, and comment density — your diff should be indistinguishable from
the existing author's.

### Honesty (the rule we care about most)
Every factual claim in your PR must be backed by a command you actually ran:

- "Tests pass" → paste the `test result:` line.
- "Clippy is clean" → paste the `Finished`/no-warnings output.
- "Fixed the bug" → show the before/after or the now-passing test.

If you **could not** verify something — a Windows-only (`#![cfg(target_os =
"windows")]`) module, a platform target you can't build, a hardware path — **say
so explicitly** and mark it as needing maintainer verification. Do not imply
coverage you don't have. A change honestly labelled "compiles on Linux; Windows
path unverified" is mergeable; a silent claim of "done" that breaks the Windows
build is not.

### Scope discipline
Make the smallest coherent change that solves the task. Do not reformat
unrelated code, bump unrelated dependencies, or "drive-by" refactor. If you spot
a separate problem, note it in the PR description — don't fold it in.

---

## 3. The workflow

1. Reproduce/understand the task against the actual code (not assumptions).
2. Make the change, matching local style and the layer/security rules above.
3. Run the **full gate** (section TL;DR). Fix anything red — for real, not by
   suppression.
4. Build every feature combination your change can affect.
5. Re-read your own diff. Remove debug prints, stray `dbg!`, commented code,
   and TODOs you introduced.
6. Write the commit and PR per section 4, backing every claim with output.

---

## 4. Commits & pull requests

- **Conventional commits**: `feat:`, `fix:`, `refactor:`, `docs:`, `chore:`,
  `test:`, `perf:`, with a scope where useful (`fix(airframe_db): …`).
- **Declare AI authorship.** If an agent wrote the change, add a trailer:

  ```
  Co-Authored-By: <Your Agent/Model> <noreply@example.com>
  ```

  and state in the PR description that AI was used and to what extent. This is
  expected here, not penalised — but it must be disclosed.
- Fill out the pull-request template (`.github/PULL_REQUEST_TEMPLATE.md`)
  honestly. Unchecked boxes are fine and informative; falsely-checked ones are
  grounds for rejection.

---

## 5. What gets a PR rejected

- A red gate, or a gate "passed" by suppressing a lint / deleting a test.
- An upward or lateral layer dependency.
- Crypto or randomness for a security decision done outside the boundary.
- A backend type (e.g. `openssl::PKey`) leaking into a public signature.
- Plaintext secrets logged or persisted.
- Claims in the PR not backed by command output, or unverifiable changes
  presented as verified.
- Unrelated reformatting/refactoring mixed into the diff.

## 6. Licence

This project is MIT-licensed. By contributing you agree your contribution is
provided under the same MIT terms.
