<!--
Before opening: read AGENTS.md (the contributor playbook) and CONTRIBUTING.md.
Check only the boxes that are genuinely true. An unchecked box is informative
and fine; a falsely-checked one is grounds for rejection.
-->

## What & why

<!-- What does this change do, and why? Link any issue. -->

## Gates (paste the real output, don't just check)

- [ ] `just fmt-check` passes
- [ ] `just clippy` is clean (`--workspace --all-targets -- -D warnings`)
- [ ] `just test` passes
- [ ] `just build` passes
- [ ] All affected feature combinations compile (list them below)

```
<paste test result: / clippy Finished lines here>
```

Feature combinations built: <!-- e.g. airframe_db --features module; default -->

## Rules checklist

- [ ] Dependencies point downward only (no upward/lateral layer edges)
- [ ] No cryptography or randomness for a security decision outside the boundary
- [ ] No crypto backend type leaked into a public API signature
- [ ] No plaintext secrets logged or persisted; security errors fail closed
- [ ] Follows `ref-project-conventions.md` (naming, layout, manifest inheritance,
      features, per-crate README)
- [ ] Scope is tight — no unrelated reformatting or dependency bumps

## Verification honesty

<!-- Anything you could NOT verify locally? (e.g. Windows-only code on Linux,
a hardware path, a target you can't build.) State it here — do not present
unverified changes as verified. -->

## AI disclosure

- [ ] An AI agent assisted with this change
- Extent / model: <!-- e.g. "Drafted by <model>, reviewed by me" -->
- [ ] A `Co-Authored-By:` trailer is present if AI-authored
