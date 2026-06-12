---
name: merge-dependabot
description: >-
  Merge open Dependabot PRs on void-cli when CI is green, the PR is at least
  48 hours old, and security checks pass. Fix failing PRs until they pass. Uses
  gh to list, check, patch, and squash-merge PRs. Use when the user asks to
  merge dependabot PRs, clear the dependabot backlog, update dependencies via
  PRs, or triage failing dependabot CI.
---

# Merge Dependabot PRs

Batch-merge Dependabot PRs on `MaximeGaudin/void` (void-cli repo root). Merge only when **all** of the following hold:

1. PR is **at least 48 hours old** (supply-chain cooling period)
2. **Security pre-checks** pass (see Step 2b)
3. **Blocking CI checks** pass; fix and push fixes on failing PRs until green

## Prerequisites

- Run all commands from the void-cli repository root (`Cargo.toml` workspace).
- `gh` authenticated with permission to merge PRs.
- Clean working tree. Stash or commit local changes before checking out PR branches.
- For local verification: Rust stable + `rustup toolchain install 1.89.0`.
- For security checks: `cargo install cargo-audit cargo-deny` (CI runs `cargo-deny`; local runs catch issues before merge).

## Blocking CI checks

These mirror `.github/workflows/ci.yml` gates. **Coverage is non-blocking** (`continue-on-error: true`) — ignore Coverage when deciding merge readiness.

| Check | Job |
|-------|-----|
| `Format` | `cargo fmt --all --check` |
| `Check (ubuntu-latest)` | clippy + test |
| `Check (windows-latest)` | clippy + test |
| `Check (macos-latest)` | clippy + test |
| `MSRV (1.89)` | `cargo check --workspace --locked` on rustc 1.89 |
| `cargo-deny` | license + advisory gate (RUSTSEC) |

## Workflow

```
Task Progress:
- [ ] Step 1: List open Dependabot PRs
- [ ] Step 1b: Filter by age (≥ 48 h)
- [ ] Step 2: Classify each eligible PR (green / failing / pending)
- [ ] Step 2b: Security pre-check (before merge)
- [ ] Step 3: Fix failing PRs
- [ ] Step 4: Merge green PRs
- [ ] Step 5: Re-check remaining PRs after merges
- [ ] Step 6: Final report
```

### Step 1 + 1b: List open Dependabot PRs and filter by age (≥ 48 h)

Only **eligible** PRs proceed to Steps 2–4. Younger PRs are **deferred** — do not merge, fix, or checkout them.

```bash
# Eligible (≥ 48 h old)
gh pr list --author "app/dependabot" --state open \
  --json number,title,headRefName,url,mergeable,createdAt \
  --jq '[.[] | select((now - (.createdAt | fromdateiso8601)) >= (48 * 3600))]
    | sort_by(.createdAt)
    | .[] | "\(.number)\t\(.title)\t\(.createdAt)\t\(.url)"'

# Deferred (< 48 h old) — report only, do not touch
gh pr list --author "app/dependabot" --state open \
  --json number,title,createdAt,url \
  --jq '[.[] | select((now - (.createdAt | fromdateiso8601)) < (48 * 3600))]
    | sort_by(.createdAt)
    | .[] | "\(.number)\t\(.title)\t\(.createdAt)\t\(.url)"'
```

Human-readable age for a single PR:

```bash
gh pr view N --json createdAt \
  --jq '"\(.createdAt)  age_hours=\(((now - (.createdAt | fromdateiso8601)) / 3600) | floor)"'
```

**Rationale:** Compromised releases are often detected within hours. Waiting 48 h lets the community and advisory databases catch malicious publishes before they land in `main`.

### Step 2: Classify CI status

For each **eligible** PR number `N`, fetch check buckets and test blocking checks only:

```bash
pr_ci_status() {
  local n="$1"
  local pending=0 failing=0
  while IFS= read -r line; do
    name="${line%%$'\t'*}"
    bucket="${line#*$'\t'}"
    case "$bucket" in
      pending) pending=1 ;;
      fail|cancel) failing=1 ;;
    esac
  done < <(gh pr checks "$n" --json name,bucket \
    --jq '.[] | select(
      .name == "Format" or
      .name == "MSRV (1.89)" or
      .name == "cargo-deny" or
      (.name | startswith("Check ("))
    ) | "\(.name)\t\(.bucket)"')

  if [ "$pending" -eq 1 ]; then echo pending
  elif [ "$failing" -eq 1 ]; then echo failing
  else echo green
  fi
}
```

- **green** → proceed to Step 2b, then Step 4 if security checks pass
- **pending** → `gh pr checks N --watch` (exit 8 = still pending), then re-classify
- **failing** → Step 3

Quick manual check: `gh pr checks N` and confirm every blocking row is `pass`.

### Step 2b: Security pre-check (before merge)

Run **before** merging any eligible, CI-green PR. A failure here blocks merge even if CI is green.

#### All PR types

- Confirm `cargo-deny` CI job is **pass** (RUSTSEC advisories + license allow-list in `deny.toml`).
- Skim the PR diff — only dependency version pins / lockfile changes expected; no unrelated workflow logic, new `permissions`, or install scripts.

#### `dependabot/cargo/*` PRs

Check out the branch and run advisory scans against the **post-bump** lockfile:

```bash
gh pr checkout N
cargo audit
cargo deny check advisories
cargo deny check licenses
```

| Result | Action |
|--------|--------|
| `cargo audit` / `cargo deny check advisories` reports **new** vulnerabilities | **Do not merge.** Fix via version bump, `deny.toml` exception (with documented rationale), or leave open for user decision. |
| Only pre-existing ignored advisories (listed in `deny.toml`) | OK to proceed |
| Unknown / unlisted license on a new crate | **Do not merge** until `deny.toml` is updated intentionally |

Optional sanity checks (use judgment, not hard gates):

- Crate name in `Cargo.toml` matches the intended upstream (watch for typo-squats).
- On crates.io, confirm the publisher and that the version is not yanked.
- For major bumps, skim the upstream changelog for suspicious or unrelated changes.

#### `dependabot/github_actions/*` PRs

Actions are a common supply-chain vector — the 48 h age gate is especially important here.

```bash
gh pr diff N
```

| Check | Action |
|-------|--------|
| Only version/tag bump on a known action (`actions/*`, `dtolnay/*`, etc.) | OK |
| New `permissions:`, `secrets:`, `env:`, or `run:` steps added | **Do not merge** — needs manual review |
| Action owner changed or unfamiliar publisher | **Do not merge** — needs manual review |
| Tag points to a commit you can verify | Prefer full-SHA pins when the workflow already uses them |

Return to `main` after checks: `git checkout main`

### Step 3: Fix failing PRs

For each **eligible**, failing PR, fix failures **in scope of the dependency bump**. Never weaken CI workflows to force green. If a failure seems unrelated, rebase onto latest `main` first — another merged PR may have already fixed it.

```bash
gh pr checkout N
```

#### 3a. Pull CI failure logs

```bash
gh pr checks N --json name,bucket,link \
  --jq '.[] | select(.bucket == "fail") | "\(.name): \(.link)"'

# Fetch the failed job log (use link from above)
gh run view <run-id> --log-failed
```

Read only the failing sections — do not dump entire JSON payloads.

#### 3b. Reproduce locally

```bash
./scripts/check.sh          # fmt + clippy + test (matches Check matrix on one OS)
cargo +1.89.0 check --workspace --locked   # MSRV gate
cargo deny check            # if installed; otherwise rely on CI
cargo audit                 # security gate
```

#### 3c. Common fix patterns

| Failure | Likely cause | Fix |
|---------|--------------|-----|
| `Format` | rustfmt drift | `cargo fmt --all`, commit |
| `Check (*)` clippy | new dep warns under `-D warnings` | fix code or adjust usage |
| `Check (*)` test | API/behavior change in bumped crate | update call sites, feature flags |
| `MSRV (1.89)` | dep needs newer rustc | bump `rust-version` in root `Cargo.toml` **and** the `1.89.0` pin in `ci.yml` MSRV job, together |
| `cargo-deny` | license or RUSTSEC advisory | update `deny.toml`, bump/pin transitive dep, or document exception |
| `MERGEABLE: CONFLICTING` | base moved | `git fetch origin main && git rebase origin/main`, resolve, push |

Major version bumps (e.g. reqwest 0.12 → 0.13) usually need source changes — read the crate changelog/migration guide.

**Do not** merge a PR that requires raising MSRV unless the failure clearly comes from the bumped dependency and the MSRV bump is intentional.

#### 3d. Push and wait for green

```bash
git add -A
git commit -m "fix(deps): make PR #N pass CI"
git push
gh pr checks N --watch --fail-fast
```

Re-run `pr_ci_status N` until `green`, then re-run Step 2b. If still failing after two focused attempts, document the blocker in the final report and move on.

### Step 4: Merge green PRs

Merge only PRs that passed Steps 1b, 2, **and 2b**. Merge order minimizes conflicts:

1. `dependabot/github_actions/*` PRs (oldest eligible first)
2. `dependabot/cargo/*` PRs (oldest eligible first)

```bash
gh pr merge N --squash --delete-branch
```

Skip PRs that are not `MERGEABLE` — rebase per Step 3c, re-check CI and security, then merge.

### Step 5: Re-check after each merge

Each merge can invalidate remaining PRs. After every merge:

```bash
gh pr list --author "app/dependabot" --state open --json number,mergeable,createdAt
```

Re-apply the 48 h filter, re-classify CI on eligible survivors, re-run security checks. Fix conflicts/rebase before merging the next one.

### Step 6: Final report

```markdown
# Dependabot Merge Report

## Merged
| PR | Title | Notes |
|----|-------|-------|
| #N | chore(deps): ... | squash-merged |

## Fixed then merged
| PR | Title | Fix applied |
|----|-------|-------------|
| #N | ... | e.g. reqwest 0.13 API migration |

## Deferred (< 48 h old)
| PR | Title | Created | Eligible after |
|----|-------|---------|----------------|
| #N | ... | 2026-06-12T10:00:00Z | ~2026-06-14T10:00:00Z |

## Still open
| PR | Title | Status | Blocker |
|----|-------|--------|---------|
| #N | ... | failing | MSRV bump needed — needs user decision |
| #N | ... | security | cargo audit: RUSTSEC-XXXX-XXXX on crate X |

## Summary
- X merged, Y fixed, Z deferred (< 48 h), W remaining
```

## Guardrails

- Only touch Dependabot PRs (`author: app/dependabot`).
- **Never merge a PR younger than 48 hours** — defer and report.
- **Never merge if Step 2b security checks fail** — document and leave for manual review.
- Never change CI workflows just to silence failures.
- Never merge with a blocking check failing or pending.
- Do not force-push except `--force-with-lease` after rebasing a Dependabot branch.
- Commit fixes on the PR branch; do not open replacement PRs unless the Dependabot branch is unrecoverable.

## Quick reference

```bash
# List eligible (≥ 48 h)
gh pr list --author "app/dependabot" --state open \
  --json number,title,createdAt \
  --jq '[.[] | select((now - (.createdAt | fromdateiso8601)) >= (48 * 3600))] | .[] | "#\(.number) \(.title)"'

# Status
gh pr checks <N>

# Security (on PR branch)
gh pr checkout <N>
cargo audit && cargo deny check advisories licenses

# Fix
./scripts/check.sh
git push

# Merge (only if ≥ 48 h old and security checks pass)
gh pr merge <N> --squash --delete-branch
```
