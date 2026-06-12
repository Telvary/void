---
name: groom-dependencies
description: Audit, update, clean up, and secure Rust (Cargo) dependencies. Use when the user asks to groom, audit, update, clean, or review dependencies, or mentions outdated crates, unused deps, dependency security, or supply-chain safety.
---

# Groom Dependencies

Full dependency grooming pass for the workspace. Note: Dependabot already opens PRs for routine bumps — to process those, use the `merge-dependabot` skill instead; this skill is for a hands-on local pass (major bumps, unused deps, alternatives, security posture).

Run from the workspace root, on a clean git tree. Track the phases with your todo list; commit at the end of each phase that changed files.

## Prerequisites

Required tools — check with `command -v cargo-outdated cargo-audit cargo-machete cargo-deny`; install missing ones with `cargo install <tool>`. Do not skip a phase because a tool is missing.

## Phase 1: Check Outdated

```bash
cargo outdated --root-deps-only
```

Classify each gap: **patch** (apply freely), **minor** (skim changelog), **major** (breaking — review carefully).

## Phase 2: Update

- Patch/minor: `cargo update` (respects `Cargo.toml` constraints, bumps `Cargo.lock` only).
- Major: edit the constraint in `[workspace.dependencies]`, but only when the new major has had time to stabilize and the migration path is clear. If a bump looks risky, flag it in the report instead of applying it.
- `cargo check --workspace` after each change to catch breakage early.

Commit: `chore(deps): update dependencies`

## Phase 3: Build & Test

`./scripts/check.sh` (fmt + clippy + tests, mirrors CI). Fix failures — version pins, code migration, feature flags — and re-run until green. Do **not** proceed while red.

## Phase 4: Remove Unused

```bash
cargo machete
```

Verify each finding (macro/build-script/feature-only uses are false positives), remove truly unused entries, re-run `./scripts/check.sh` after each removal.

Commit: `chore(deps): remove unused dependencies`

## Phase 5: Suggest Alternatives (report only — never auto-apply)

Flag crates that are unmaintained (2+ years silent, archived), have a widely-adopted successor, are replaceable by `std` at our MSRV, or are much heavier than the features we use. For each: alternative, migration effort (trivial/moderate/significant), trade-offs.

## Phase 6: Security Audit

```bash
cargo audit
cargo deny check advisories
cargo deny check licenses    # allow-list lives in deny.toml; CI enforces this
```

For each advisory: note the RUSTSEC ID, severity, affected crate; check whether a bump resolves it; otherwise document the workaround or risk acceptance. Commit any fixes: `chore(deps): fix security advisories`

## Phase 7: Final Report

```markdown
# Dependency Grooming Report
## Updates Applied        | Crate | Old | New | Type |
## Unused Removed         - crate — why
## Suggested Alternatives | Current | Suggested | Effort | Reason |
## Security               | Advisory | Crate | Severity | Status |
## Summary                X updated / X removed / X suggestions / X advisories (fixed/remaining)
```

Include every section, with "None" where empty, so the user knows it was checked. If the updates are user-visible, add a `**Dependencies**` bullet under `[Unreleased]` in `CHANGELOG.md`.
