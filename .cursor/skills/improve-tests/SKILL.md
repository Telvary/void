---
name: improve-tests
description: Fix broken tests, audit existing tests for validity and coverage, identify missing tests across all crates, add them, and ensure the full suite passes. Use when the user asks to improve tests, add missing tests, fix failing tests, increase test coverage, or audit test quality.
---

# Improve Tests

Fix, audit, and expand the test suite. Read [docs/testing.md](../../../docs/testing.md) first — it describes the suite layout, conventions, and known coverage gaps.

Track the phases with your todo list. Commit at the end of each phase that changed files (`test: <what>` or `fix: <what>` for code bugs).

---

## Phase 1: Fix Broken Tests

Run `./scripts/check.sh` (fmt + clippy `--all-targets` + tests, mirrors CI). For each failure, decide whether the **test** or the **code** is wrong — fix the code if it's a bug, update the test if the expectation is stale. Re-run until green.

## Phase 2: Audit Existing Tests

For every `#[cfg(test)]` module and every file in `crates/void-cli/tests/`:

| Check | Action if failing |
|-------|-------------------|
| **Correctness** — asserts the right behavior? | Fix assertion logic |
| **Relevance** — tested code still exists/behaves this way? | Update or remove stale tests |
| **Isolation** — depends on network or shared state? | In-memory DB or unique temp dirs |
| **Naming** — describes the scenario? | Rename to `<function>_<scenario>` |
| **Edge cases** — happy path only? | Note gaps for Phase 3 |

## Phase 3: Identify Missing Tests

Walk each crate's public API and note untested happy paths, error paths, and boundary conditions. Prioritize:

1. **High** — pure logic: parsing, mapping, formatters, config (de)serialization, DB queries
2. **Medium** — trait impls with branching logic
3. **Low** — network-calling API clients (need HTTP mocking), end-to-end flows

Produce the list grouped by crate/module before writing any code.

## Phase 4: Add Missing Tests

Follow the existing conventions of this workspace:

- **Unit tests** live in `#[cfg(test)] mod tests` co-located with the code (large modules use a sibling `tests.rs`); **CLI integration tests** live in `crates/void-cli/tests/` (`cli_contract.rs`, `first_run.rs`, `read_paths.rs`)
- Std test framework only; `#[tokio::test]` for async code; new dev-deps only if truly necessary
- Database tests: `Database::open_in_memory()`. File I/O tests: `std::env::temp_dir()` + a `uuid::Uuid::new_v4()` subdirectory, cleaned up at the end
- Name pattern `<function_under_test>_<scenario>` (e.g. `expand_tilde_no_home`); one behavior per test; cover both success and error paths

## Phase 5: Final Verification

`./scripts/check.sh` must be fully green. Then summarize: tests fixed / updated / removed / added (by crate), and any code bugs found and fixed along the way.
