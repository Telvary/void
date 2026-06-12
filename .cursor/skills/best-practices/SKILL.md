---
name: best-practices
description: Scan a Rust codebase for bad practices, fix anti-patterns, split large files, reorganize modules by business domain, and apply idiomatic Rust conventions. Use when the user asks to clean up code, refactor, apply best practices, reorganize modules, reduce file size, or improve code quality.
---

# Best Practices — Rust Codebase Refactoring

Audit and refactor the workspace in five phases. Full anti-pattern catalog: [rust-checklist.md](rust-checklist.md).

Track the phases with your todo list. After every change set, run `./scripts/check.sh` (fmt + clippy + tests, mirrors CI exactly) and fix regressions before moving on. Commit at the end of each phase that changed files:

```bash
git add -A && git commit -m "refactor: <what this phase did>"
```

---

## Phase 1: Audit

Produce a findings report **before changing anything**.

1. **Automated checks** — `./scripts/check.sh` (capture clippy/fmt output).
2. **Manual scan** — read the `.rs` files and check against [rust-checklist.md](rust-checklist.md).
3. **Duplication scan** — copy-pasted logic across crates, repeated types that belong in `void-core`, boilerplate (error handling, API wrappers, mapping) that wants a shared trait/helper, duplicated constants.
4. **Placeholder & stub scan** — code that looks implemented but does nothing is **Critical**:

   ```bash
   rg -in "todo|fixme|hack|xxx|stub|placeholder|unimplemented|to be implemented|future:|later:" --type rust
   ```

   Also flag: `todo!()`/`unimplemented!()` (panics at runtime), empty match arms or loop bodies that silently drop events, functions returning hardcoded values or bare `Ok(())` instead of doing real work, no-op poll/tick handlers. Each must be implemented or explicitly removed with rationale.
5. **File size audit** — list non-test files over 400 lines (split candidates for Phase 3):

   ```bash
   find crates -name "*.rs" -not -name "tests.rs" -not -path "*/tests/*" | xargs wc -l | sort -rn | head -20
   ```

Severity levels: **Critical** (correctness/safety — fix immediately), **Warning** (anti-pattern — fix this pass), **Info** (style — fix if nearby).

Present the findings as a table (`File | Severity | Finding | Proposed fix`) and **wait for user confirmation** before Phase 2.

---

## Phase 2: Fix Anti-Patterns

Work through the findings in dependency order: `void-core` first, then connector crates, then `void-cli`.

Highest-value fixes (full list in [rust-checklist.md](rust-checklist.md)):

- `thiserror` types where errors cross crate boundaries; `anyhow` only in the binary crate
- Replace `.unwrap()`/`.expect()` in non-test code with `?` propagation
- `&str`/`&[T]` parameters instead of owned `String`/`Vec` when the callee only reads
- Remove needless `clone()`; use `std::mem::take`/`Option::take` over clone-and-reassign
- Extract duplicated logic into shared functions, traits, or `void-core`
- Remove unused dependencies (`cargo machete`)

---

## Phase 3: Split Large Files

Target: no non-test `.rs` file over ~300 lines. For each candidate from Phase 1.5:

1. Identify cohesive groups (section comments, impl blocks per type, logically grouped functions)
2. Extract each group into a submodule: `src/foo.rs` → `src/foo/{mod.rs,bar.rs,baz.rs}`
3. Keep the public API unchanged — `mod.rs` contains only `mod` declarations and curated `pub use` re-exports

Run `./scripts/check.sh` after each split; the public API must not change.

---

## Phase 4: Reorganize by Business Domain

- **void-core**: only domain models, traits, config, persistence — no connector logic
- **Connector crates**: only that connector's API client, auth, mapping, and `Connector` impl
- **void-cli**: only CLI parsing, output formatting, orchestration — no business logic
- Shared types live in `void-core`, never duplicated across connectors; no circular deps
- Module names match domain concepts (`auth`, `sync`, `api`, `models`); no `utils.rs`/`helpers.rs` catch-alls
- When moving types between crates, update imports and run `cargo check --workspace` after each move

---

## Phase 5: Polish

- Doc comments (`///`) on all public items
- Consistent `#[derive]` order (`Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize`) and `use` grouping (std → external → internal → `self`/`super`)
- `#[derive(Default)]` over manual impls when possible; private fields + constructor where invariants exist

Final gate — all must pass with zero warnings:

```bash
./scripts/check.sh && cargo build --release
```
