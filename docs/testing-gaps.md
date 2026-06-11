# Test hardening roadmap

Void has ~600 unit tests today, but they are concentrated in the database and data-model layers. This document inventories what is **missing** for the harness to reliably go red when something breaks — a contribution, a Dependabot bump, or a refactor. It is ordered by risk: P0 items can break users silently today.

Snapshot of current coverage (src lines vs. test functions, inline `#[cfg(test)]` only — there are **no** `tests/` integration directories anywhere):

| Crate | src lines | tests | Well covered | Thin or absent |
|-------|----------:|------:|--------------|----------------|
| void-core | ~10,800 | 256 | DB queries, FTS5 search, dedup, mute patterns, hook I/O, placeholders | sync engine, schema migrations, remote SSH execution |
| void-cli | ~9,150 | 130 | calendar date parsing, pagination, connector filter resolution | binary-level behavior, send/doctor/forward, output formatting |
| void-slack | ~4,200 | 37 | API response mapping, manifest | api.rs (741 l), socket_mode.rs (478 l), sync.rs |
| void-gmail | ~3,200 | 41 | RFC 2822 composition, reply-all | api.rs (766 l), history sync |
| void-linkedin | ~3,100 | 39 | wiremock API integration, extraction | full sync flow |
| void-calendar | ~2,000 | 22 | event mapping, attendees | api.rs (519 l), sync ops |
| void-whatsapp | ~1,800 | 37 | JID/message extraction | sync.rs (360 l), connector trait |
| void-telegram | ~1,500 | 15 | errors, session, send | connector trait (396 l), sync — **1 test total on the connector** |
| void-gdrive | ~780 | 18 | URL parsing, export formats | connector trait |
| void-hackernews | ~470 | 6 | keyword/score filtering | api.rs |

Existing infrastructure to build on: `wiremock` (already a dev-dep in 5 connector crates), `Database::open_in_memory()` used throughout void-core, builder-style test data helpers. Missing entirely: `assert_cmd`/`trycmd` (binary tests), `insta` (snapshots), fixture files, coverage measurement.

## P0 — The harness itself (CI gaps that let breakage through)

These are not tests but holes in the net. Cheapest wins in the document:

- [ ] **macOS is not in the CI matrix.** CI tests `ubuntu-latest` and `windows-latest`, but releases ship four Darwin/Linux targets. A macOS-only breakage (path handling, keychain, `~/Library` config fallback) reaches a release untested. Add `macos-latest` to the `check` matrix.
- [ ] **`cargo deny` is configured but never runs.** `deny.toml` exists; no CI job executes it. Dependabot can merge a dependency with a yanked version, RUSTSEC advisory, or GPL-incompatible license without anything going red. Add an `EmbarkStudios/cargo-deny-action` job.
- [ ] **No MSRV check.** `rust-version = "1.75"` is declared but CI builds on `stable` only. Any dependency bump that needs a newer rustc breaks downstream `cargo install` silently. Add a job pinned to 1.75 running `cargo check --locked`.
- [ ] **CI doesn't use `--locked`.** `cargo test` may resolve newer semver-compatible versions than `Cargo.lock`, so CI doesn't test what a release builds. Use `cargo test --locked` everywhere (CI and release).
- [ ] **No coverage signal.** Add a `cargo llvm-cov` job uploading to Codecov — not for a vanity badge, but so PR reviews see when a contribution adds untested surface.

## P0 — Binary-level integration tests (the missing layer)

Nothing today executes the `void` binary. A clap upgrade via Dependabot that changes parsing semantics, a panic in command dispatch, or a broken `--help` would pass `cargo test` green. Create `crates/void-cli/tests/` with `assert_cmd` + `predicates` + `tempfile`:

- [ ] **CLI contract tests** — for every top-level command and subcommand: `void <cmd> --help` exits 0; required-arg violations exit non-zero with a useful message (`send` without `--via`, `archive` with neither ids nor `--before`, `--before` combined with ids, invalid `--connector` values).
- [ ] **Read-path smoke tests against a seeded store** — point `--store`/`--config` at a tempdir with a seeded SQLite database (reuse the void-core test builders): `inbox`, `search`, `messages`, `conversations`, `contacts`, `channels`, `mute --list` produce expected rows and exit 0. This single fixture catches schema/query/output regressions across the whole read surface.
- [ ] **First-run behavior** — `void inbox` with no config: auto-creates default config (current behavior) and doesn't panic; `void doctor --non-interactive` exits with a documented code.
- [ ] **Snapshot the output formats** (`trycmd` or `insta`) for `inbox`, `search`, and `calendar` table rendering — output is the API for scripts and agent skills; today any formatting change is invisible to tests.

## P0 — Sync engine orchestration (`void-core/src/sync.rs`)

3 tests exist, all on file locking. The orchestrator itself (~240 lines) is untested:

- [ ] Engine spawns one loop per configured connection and isolates failures (one connector erroring doesn't kill the others — the `--allow-broken` contract).
- [ ] Graceful shutdown: cancellation token stops loops and releases the `LOCK` file.
- [ ] A mock `Connector` trait implementation (in-crate test double) that records calls — enables all of the above without network.
- [ ] Hook runner attachment: a synced message actually reaches trigger evaluation.

## P0 — Schema migrations

One test asserts the final schema version. Nothing executes the upgrade *path*:

- [ ] For each migration N: open an in-memory DB at schema N−1 (committed SQL fixture), run migrations, assert success **with data present** — migrations on empty tables hide `ALTER TABLE` mistakes.
- [ ] Snapshot test of the final schema (`SELECT sql FROM sqlite_master`) so schema drift shows up in diffs.
- [ ] `bulk_archive_before`: now publicly documented, zero direct tests — boundary timestamp, connector filter, muted/archived interaction, empty result.

## P1 — Hook execution chain

Hook file I/O, placeholders, and active windows are tested; the *chain that fires them* is not:

- [ ] Trigger evaluation: `new_message` with/without connector filter; `schedule` cron due/not-due (the cron evaluation has **zero** tests — inject the clock, don't call `Utc::now()` in the evaluator).
- [ ] `execute_hook` against a **stub agent binary** (a shell script on `PATH` emitting canned stream-json): success parse, error-in-stream extraction, non-zero exit, garbage output. Today only the stream-error extraction helper is tested.
- [ ] `hook test --message-id` end-to-end against the stub agent (binary-level, builds on the P0 harness).

## P1 — Connector sync flows over wiremock

`wiremock` is already in place in 5 crates but mostly exercises single API calls. Extend to the **sync loop** level, per connector:

- [ ] Happy-path incremental sync: mock server returns 2 pages of messages → DB contains them, cursor/syncToken persisted (Gmail `history.list` pagination has a known bug class here — it was fixed in 0.9.x and has no regression test).
- [ ] Error paths as fixtures: HTTP 401 (token expired → re-auth path), 429 (rate-limit → backoff, not crash), 5xx, malformed/missing-field JSON. One fixture corpus per connector under `tests/fixtures/`.
- [ ] **void-telegram is the outlier**: 1 connector test for ~400 lines of trait implementation. Its message-extraction logic deserves the same treatment void-whatsapp already has (37 extraction tests).

## P1 — Remote store

Only path/quoting helpers and cache TTL math are tested. The proxy machinery is not:

- [ ] Inject a **fake `ssh`/`scp`** (script on `PATH` that records argv and replays canned output) to test: command proxying argv construction, attachment staging order (SCP before remote exec), download pull-back, unreachable-host error surfacing.
- [ ] Cache lifecycle: snapshot older than `database_ttl_secs` triggers refresh; corrupted cached snapshot recovers instead of crashing; `remote refresh` invalidates both config and DB.

## P2 — Edges that bite later

- [ ] Config migration chains: legacy `[[accounts]]` + sidecar Slack token + old store paths, loaded together; unknown connector `type` produces a clear error, not a serde panic.
- [ ] Date/time boundaries: DST transitions for calendar event creation and `parse_schedule_time` (both use `Local`), `--before` at midnight boundaries, active-window wrap (`22:00 → 06:00`) at exactly the endpoints (end is exclusive).
- [ ] Placeholder edge cases: unknown tokens left intact, `{message}` containing `{now}`-like text (expansion-order injection), very large messages.
- [ ] FTS5 property test (`proptest`): arbitrary user input to `void search` never produces an SQL/FTS syntax error.
- [ ] Output of `--page` past the last page, `-n 0`, and conflicting flags.

## Suggested sequencing

1. **CI net first** (macOS, `cargo deny`, MSRV, `--locked`) — one PR, no test code, immediately catches whole classes of breakage including Dependabot regressions.
2. **Binary harness + seeded-store fixture** — the highest leverage new code; every future feature gets a cheap place to add a test.
3. **Sync engine mock connector + schema migration fixtures** — protects the data layer.
4. Hook chain, connector error corpus, remote fakes, then P2.

New dev-dependencies this implies: `assert_cmd`, `predicates`, `tempfile`, `trycmd` *or* `insta` (pick one), `proptest` (P2 only). All test-only — no impact on the shipped binary.
