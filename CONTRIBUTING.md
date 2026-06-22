# Contributing to Airframe

Thanks for your interest in contributing. Airframe is a security-focused,
modular Rust framework that was largely built with AI assistance and is designed
to be worked on the same way.

## AI-assisted contributions are welcome

You may use an LLM or coding agent to prepare your contribution — in fact it's
encouraged. **If you do, point your agent at [`AGENTS.md`](AGENTS.md) first.**
That file is the strict, machine-readable playbook: the build/lint/test gates,
the layering rules, the security model, and the honesty requirements your change
must satisfy. A PR produced by an agent that followed `AGENTS.md` is exactly what
we want; one produced by an agent that skipped it is usually obvious and usually
bounced.

Whether human- or AI-authored, every contribution is held to the same bar.

## Quick start

```bash
git clone <this-repo> && cd airframe
just build        # cargo build --workspace
just test         # cargo test --workspace
```

Make your change, then run the full gate — **all must be green**:

```bash
just fmt-check    # formatting
just clippy       # cargo clippy --workspace --all-targets -- -D warnings
just test         # cargo test --workspace
just build        # cargo build --workspace
```

Or run the whole gate — including every advertised feature combination and the
docs — with one recipe:

```bash
just release-check
```

**This project has no CI, on purpose** — a supply-chain–risk decision (no
third-party runners or marketplace actions). The gate is run manually, and the
maintainer re-runs `just release-check` before merging. Paste its real output
into your PR; an honest red result beats a green claim that doesn't reproduce.

## The hard rules (summary — see [`AGENTS.md`](AGENTS.md) for detail)

1. **Gates green, no suppression.** Don't silence a lint or delete a test to pass.
2. **Layering is downward-only.** A crate may depend on lower layers, never
   higher. See [`docs/arch-design.md`](docs/arch-design.md).
3. **Crypto stays at the boundary.** No ad-hoc cryptography or randomness for
   security decisions; never leak a crypto backend type in a public API; never
   log or store plaintext secrets; fail closed.
4. **Follow the conventions.** See
   [`../spacetime/docs/ref-project-conventions.md`](../spacetime/docs/ref-project-conventions.md).
5. **Be honest.** Back every "it works" claim with the command output. State
   plainly anything you couldn't verify (e.g. Windows-only code on a Linux host).
6. **Keep scope tight.** Smallest coherent change; no drive-by reformatting.

## Opening a pull request

- Use conventional-commit messages (`fix(scope): …`).
- Fill out the PR template honestly.
- **Disclose AI use.** If an agent helped, say so and add a
  `Co-Authored-By:` trailer. This is expected, not penalised.

## Reporting security issues

Please **do not** open a public issue for a security vulnerability. Disclose it
privately to the maintainer first.

## Licence

Airframe is licensed under the [MIT License](LICENSE). By contributing, you
agree that your contributions are licensed under the same MIT terms.
