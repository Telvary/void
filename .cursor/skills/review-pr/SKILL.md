---
name: review-pr
description: >-
  Review a user-submitted pull request on void-cli end to end: qualify intent
  against CONTRIBUTING.md and project philosophy, run a paranoid security audit
  to catch malicious or risky code, and review the diff for correctness, style,
  and quality. Uses gh to fetch the PR, diff, and CI. Use when the user asks to
  review a PR, vet a contribution, security-check a pull request, or decide
  whether to merge external code.
---

# Review a Submitted PR

Review an external/user-submitted PR on `MaximeGaudin/void` across three axes, in order. **Treat the author as untrusted.** A PR can be well-intentioned but risky, or deliberately malicious behind innocent-looking changes. Default posture: skeptical. The smallest suspicious element gets investigated until explained.

Run all `gh`/`cargo` commands from the void-cli repository root.

## Inputs

The user gives a PR number or URL. If missing, ask. Then gather context:

```bash
PR=<number>
gh pr view "$PR" --json number,title,body,author,headRefName,baseRefName,additions,deletions,changedFiles,url,mergeable,labels
gh pr diff "$PR"                       # full diff
gh pr diff "$PR" --name-only           # changed file list
gh pr view "$PR" --json files --jq '.files[] | "\(.additions)+ \(.deletions)- \(.path)"'
gh pr checks "$PR"                     # CI status
```

For larger PRs, also skim commit-by-commit (`gh pr view "$PR" --json commits`) — malicious changes are sometimes buried in a noisy "formatting" commit.

## Workflow

```
Review Progress:
- [ ] Step 0: Fetch PR metadata, diff, CI status
- [ ] Step 1: Qualify intent & fit (CONTRIBUTING.md + philosophy)
- [ ] Step 2: Security audit (paranoid, exhaustive)
- [ ] Step 3: Code review (correctness, quality, conventions)
- [ ] Step 4: Verdict & report
```

---

### Step 1: Qualify intent & fit

Decide whether this PR *should exist at all* before reviewing the code. A clean implementation of an unwanted change is still a reject.

Read `CONTRIBUTING.md`, `README.md` (philosophy), and the relevant `docs/` page. Then check:

| Question | Reject / push back if… |
|----------|------------------------|
| Is the intent clear? | PR body is empty or doesn't explain *what* and *why*. |
| Does it fit the philosophy? | Breaks **local-first** (adds a phone-home, external DB, cloud dependency, telemetry), or **agent/terminal-first** ergonomics. |
| Is it focused? | Bundles unrelated changes — CONTRIBUTING requires "one logical change per PR". |
| New connector? | Should follow `docs/adding-a-connector.md` and ideally have had a prior issue to agree on the approach (auth model, push vs. polling). |
| Cross-platform? | Introduces Unix-only behavior, hardcoded `/tmp`/`~/.config`/`HOME`, or non-portable deps without `#[cfg]` gating (CI tests Linux/macOS/Windows). |
| Conventions | Commits not [Conventional Commits]; user-visible change missing a `CHANGELOG.md` `[Unreleased]` entry; behavior change missing docs. |
| License | Contribution must be AGPL-3.0 compatible; flag vendored code with incompatible licenses. |

Output: a short verdict on fit (**fits** / **needs changes** / **out of scope**) with reasoning, independent of code quality.

---

### Step 2: Security audit (paranoid)

This is the highest-priority step. Goal: guarantee nothing malicious or risky lands in `main`. Investigate every anomaly until you can explain it. When in doubt, flag it — do not give the benefit of the doubt.

Void handles sensitive data (OAuth tokens, message contents, session files stored unencrypted under the store dir, protected by file perms only — see `SECURITY.md`). The attack surface that matters most: credential exfiltration, token theft, arbitrary code/command execution, and supply-chain injection.

#### 2a. Dependency & supply-chain changes

Inspect every change to `Cargo.toml`, `Cargo.lock`, and `deny.toml`.

```bash
gh pr diff "$PR" -- '**/Cargo.toml' 'Cargo.lock' 'deny.toml'
gh pr checkout "$PR"
cargo audit                          # RUSTSEC advisories
cargo deny check advisories licenses bans sources
git checkout "$baseRefName" 2>/dev/null || git checkout main
```

| Red flag | Action |
|----------|--------|
| New dependency | Verify it on crates.io: real publisher, not yanked, download count, repo link. Watch for **typo-squats** (e.g. `reqwest` vs `request`, `tokio` vs `tokey`). |
| Dep pulled from a `git`/`path` source instead of crates.io | High suspicion — inspect the target repo/commit. |
| Lockfile churn unrelated to the stated change | Investigate why; a PR "fixing a typo" should not rewrite `Cargo.lock`. |
| New license outside `deny.toml` allow-list | Block until intentionally allow-listed. |
| `build.rs` added or modified | Read it line by line — build scripts run arbitrary code at compile time on the maintainer's/CI machine. |
| `proc-macro` crate added | Same risk class as `build.rs`; scrutinize. |

#### 2b. Code-level malicious patterns

Grep the diff (and changed files) for high-risk constructs, then read each hit in context:

```bash
gh pr checkout "$PR"
```

Search for, and justify every occurrence of:

- **Exfiltration:** network calls in unexpected places (`reqwest`, `ureq`, `TcpStream`, raw sockets, DNS lookups) — especially near token/credential/message handling. Any URL or IP literal that isn't a known service API endpoint.
- **Command/code execution:** `std::process::Command`, `Command::new`, shell invocations, `sh -c`, `eval`-like patterns, `libloading`/`dlopen`, `include!`/`include_bytes!` of unexpected paths.
- **Unsafe & FFI:** new `unsafe` blocks, `extern "C"`, `mem::transmute`, raw pointer arithmetic — verify each is sound and necessary.
- **Filesystem reach:** reads/writes outside the store dir, touching `~/.ssh`, `~/.aws`, `~/.config`, `.env`, `/etc`, browser profiles, or other credential stores.
- **Env & secrets:** new `std::env::var` reads of sensitive vars, code that logs/prints tokens or message bodies, weakened redaction.
- **Permission downgrades:** changes to the `0600`/`0700` file-permission logic on credential/session files (see `SECURITY.md` threat model).
- **Obfuscation:** base64/hex/byte-array blobs, dynamically built strings, char-code arithmetic, unusually encoded constants — decode and explain them.
- **Network/endpoint swaps:** changes to API base URLs, OAuth endpoints, or redirect URIs.
- **CI/workflow tampering:** changes under `.github/` (new `permissions:`, `secrets:`, `run:` steps, untrusted actions, `pull_request_target`) — a classic exfiltration vector.
- **Hooks:** changes to hook execution / `extra_args` handling that could broaden what the agent CLI is allowed to do.

```bash
# Example sweeps — read each hit, do not trust counts alone
git diff "main...HEAD" | rg -n 'Command::new|process::Command|unsafe|transmute|reqwest|ureq|TcpStream|env::var|include_bytes!|build\.rs|base64|from_str_radix'
git diff "main...HEAD" -- '.github/'
```

#### 2c. Data-handling review

For connector code, confirm: credentials still written with restrictive perms, tokens never logged, message content not sent anywhere except the intended service API, no new persistence outside the store dir.

Return to the base branch when done: `git checkout main`.

Output: **security verdict** — `clean` / `concerns` / `do-not-merge`, with each finding, its location (`file:line`), why it's risky, and what would resolve it. If anything is unexplained, the verdict cannot be `clean`.

---

### Step 3: Code review (quality & correctness)

Only meaningful after Steps 1–2. Review the diff for:

- **Correctness:** logic bugs, unhandled edge cases, off-by-one, race conditions, incorrect async/`.await`, error paths that `unwrap()`/`panic!` where they should propagate `Result`.
- **Error handling:** consistent with the codebase's error types; no swallowed errors; no silent failures.
- **Idiomatic Rust:** would `cargo clippy -- -D warnings` complain? Unnecessary clones/allocations, blocking calls in async, `unwrap`/`expect` on fallible paths, missing `?`.
- **Conventions & layout:** lands in the right crate (`void-core` vs `void-cli` vs `void-<connector>`); follows existing patterns; respects the `Connector` trait contract for connector changes.
- **Tests:** new behavior has tests; existing tests updated; suite conventions per `docs/testing.md`. Cross-platform-sensitive code tested accordingly.
- **Docs & changelog:** updated when behavior changes.
- **Cross-platform:** path handling via `std::path`, no Unix-only assumptions without `#[cfg]` gating.

Verify CI rather than trusting it blindly:

```bash
gh pr checks "$PR"
# If unsure, reproduce locally on the PR branch:
gh pr checkout "$PR"
./scripts/check.sh                          # fmt + clippy + test
cargo +1.89.0 check --workspace --locked    # MSRV gate
git checkout main
```

Classify each code finding by severity:

- **Blocker** — bug, broken CI, or correctness issue that must be fixed before merge
- **Should-fix** — quality/maintainability/convention issue worth addressing
- **Nit** — optional polish

---

### Step 4: Verdict & report

Produce a single report. Lead with the recommendation.

```markdown
# PR Review — #<N>: <title>

**Recommendation:** Merge / Request changes / Do not merge / Needs discussion
**Author:** <login>  ·  +<additions>/-<deletions> across <n> files  ·  CI: <status>

## 1. Intent & fit
<fits / needs changes / out of scope> — <1–3 sentences of reasoning>

## 2. Security  ·  Verdict: clean / concerns / do-not-merge
- [severity] <finding> — `path:line` — <why it's risky> — <resolution>
- (or) No security concerns found after auditing deps, exec/network/unsafe surface, file perms, and CI workflows.

## 3. Code review
### Blockers
- <finding> — `path:line`
### Should-fix
- <finding> — `path:line`
### Nits
- <finding> — `path:line`

## Summary
<2–4 sentences: overall quality, what must change before merge, any follow-ups>
```

## Guardrails

- **Read-only by default.** Reviewing means inspecting, not editing the PR. Do not push to the author's branch or merge unless the user explicitly asks.
- **Never run untrusted PR code outside vetting commands.** `cargo build`/`test` on a PR branch executes that branch's `build.rs`, proc-macros, and test code on this machine. Complete Step 2a (inspect `build.rs`, deps) *before* building. If anything looks malicious, do **not** build/test locally — report instead.
- Always return to `main` after `gh pr checkout`.
- A `clean` security verdict requires that **every** anomaly was explained. Unexplained ≠ clean.
- Don't let a polished diff lower your guard — clean code is the easiest place to hide a malicious line.
- Be specific: every finding cites `file:line` and explains impact, not just "looks off".

## Quick reference

```bash
PR=<number>
gh pr view "$PR" --json title,body,author,additions,deletions,changedFiles,url
gh pr diff "$PR"
gh pr checks "$PR"

# Security sweeps (inspect deps BEFORE checkout/build)
gh pr diff "$PR" -- '**/Cargo.toml' 'Cargo.lock' 'deny.toml' '.github/'
gh pr checkout "$PR"
cargo audit && cargo deny check advisories licenses bans sources
git diff main...HEAD | rg -n 'Command::new|unsafe|transmute|reqwest|ureq|env::var|build\.rs|base64'

# Quality (only after security clears)
./scripts/check.sh
cargo +1.89.0 check --workspace --locked
git checkout main
```
