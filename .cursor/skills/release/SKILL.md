---
name: release
description: Prepare, tag, and publish a new release — bump version, update CHANGELOG.md, build/install via project script, commit, create annotated tag, and optionally push + trigger CI release workflow when requested. Use when the user asks to release, tag, cut a release, bump version, update the changelog, or publish.
---

# Release

Step-by-step procedure for cutting a new Void CLI release.

## 1. Determine the new version

Read the current version:

```
grep '^version' Cargo.toml   # workspace version
git tag --list --sort=-v:refname | head -5
```

Choose the next version following [Semantic Versioning](https://semver.org):

| Change type | Bump |
|---|---|
| Breaking CLI/trait changes | Major (X.0.0) |
| New features, commands, connectors | Minor (0.X.0) |
| Bug fixes, refactors, dependency updates | Patch (0.0.X) |

## 2. Gather changes since last tag

```
git log <LAST_TAG>..HEAD --oneline --reverse
```

Categorize every commit into **Added**, **Changed**, **Fixed**, **Removed** sections per [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## 3. Update CHANGELOG.md

Open `CHANGELOG.md` at the project root. Insert a new section **above** the previous release:

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Added
- ...

### Changed
- ...

### Fixed
- ...
```

Rules:
- Changelog bullets use past tense ("Added", "Fixed") regardless of commit-message tense.
- Group by category, not by crate. Prefix with the scope in bold when relevant (e.g. `**Gmail** — ...`).
- Each bullet must be user-facing and understandable without reading the code.
- Do NOT include merge commits, CI-only changes, or trivial reformatting.
- Fold any pending `## [Unreleased]` entries into the new section (and leave an empty `## [Unreleased]` heading behind).

## 4. Bump the workspace version

Edit `Cargo.toml` (root workspace):

```toml
[workspace.package]
version = "X.Y.Z"
```

All crates inherit `version.workspace = true`, so a single edit propagates everywhere.

## 5. Pre-flight checks (mirror CI)

Run every check that CI enforces **before** committing, so failures are caught locally:

```bash
./scripts/check.sh          # runs fmt + clippy + test with the same flags as CI
```

Windows:

```powershell
.\scripts\check.ps1
```

The script mirrors `.github/workflows/ci.yml` exactly (sets `RUSTFLAGS=-D warnings`, runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`). Catching failures here avoids a push → fail → amend → force-push cycle.

> `./scripts/build-install.sh` (step 6) already calls `check.sh` automatically before building. Pass `--skip-checks` / `-SkipChecks` to bypass it when iterating quickly.

## 6. Build and install

```
./scripts/build-install.sh
void --version   # confirm new version
```

Windows:

```powershell
.\scripts\build-install.ps1
void --version
```

Important:
- **Always** use project install scripts. Do not use `cp` or manual binary copy.
- The script safely stops the sync daemon and performs an atomic replace.

## 7. Commit and tag

```
git add -A
git commit -m "chore: release vX.Y.Z"
git tag -a X.Y.Z -m "Release X.Y.Z"
```

Use an **annotated** tag (`-a`), not a lightweight tag.

## 8. Verify

```
git log --oneline -1
git tag -l "X.Y.Z" -n1
void --version
```

## 9. Publish (only when explicitly requested)

If the user asks to publish the release, push the commit and tag:

```bash
git push origin HEAD
git push origin X.Y.Z
```

Then trigger the CI release workflow, which builds cross-platform binaries and creates the GitHub release automatically:

```bash
gh workflow run release.yml -f tag=X.Y.Z
```

Watch the workflow to completion:

```bash
gh run watch $(gh run list --workflow=release.yml --limit=1 --json databaseId --jq '.[0].databaseId')
```

The CI workflow (`release.yml`) handles:
- Building binaries for macOS (arm64/amd64), Linux (arm64/amd64), and Windows (amd64)
- Extracting the changelog section for the release notes
- Creating (or updating) the GitHub release with all artifacts attached
- Updating the Homebrew formula in `MaximeGaudin/homebrew-tap` — confirm this job didn't emit the "deploy key not set" warning, and spot-check the tap was bumped to X.Y.Z

## Checklist

Copy and track:

```
Release X.Y.Z:
- [ ] Determine version number
- [ ] Gather commits since last tag
- [ ] Update CHANGELOG.md
- [ ] Bump version in Cargo.toml
- [ ] Pre-flight: ./scripts/check.sh (fmt + clippy + test, matches CI)
- [ ] Build and install locally (./scripts/build-install.sh)
- [ ] void --version shows new version
- [ ] git commit
- [ ] git tag -a X.Y.Z
- [ ] Verify tag
- [ ] (If requested) Push commit and tag
- [ ] (If requested) Trigger CI release workflow
```

## Notes

- Use `./scripts/build-install.sh` (or `build-install.ps1` on Windows) instead of manual copy.
- Do NOT push commits/tags or trigger the CI release unless the user explicitly asks.
- The GitHub release and cross-platform binaries are created by CI (`release.yml`), not locally. Always use `gh workflow run` to trigger it.
