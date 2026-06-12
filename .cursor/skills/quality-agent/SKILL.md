---
name: quality-agent
description: Run a full codebase quality sweep by launching parallel sub-agents for dependency grooming, test improvement, and best-practices refactoring. Use when the user asks for a quality check, full audit, codebase health review, or wants to run all maintenance tasks at once.
---

# Quality Agent

Orchestrate a full quality sweep: three specialist sub-agents in parallel, then a docs sync check and a final verification.

Verify the git tree is clean first (`git status --porcelain`); if not, warn the user before proceeding.

## Step 1: Launch sub-agents

Spawn three sub-agents **in parallel** (Claude Code: Agent tool; Cursor: parallel agents). If your environment cannot run sub-agents, execute the three skills sequentially yourself in this order: groom-dependencies → improve-tests → best-practices.

| Sub-agent | Skill file to inline in its prompt |
|-----------|-----------------------------------|
| Dependency Groomer | `.cursor/skills/groom-dependencies/SKILL.md` |
| Test Improver | `.cursor/skills/improve-tests/SKILL.md` |
| Best Practices Auditor | `.cursor/skills/best-practices/SKILL.md` |

Prompt template for each:

> You are working on the Rust workspace at `<workspace_path>`. Follow the skill instructions below to completion, tracking progress with your todo list. Return a concise report: changes made, issues found, items needing user attention.
>
> `<full contents of the SKILL.md>`

Tell the **Best Practices Auditor** to skip its Phase 1 user-confirmation gate (this is an automated sweep) but still include the findings table in its report.

## Step 2: Docs sync check

After the sub-agents finish, cross-reference the docs against the code:

- `README.md` and `docs/commands.md` vs the `Command` enum in `crates/void-cli/src/main.rs` and each subcommand module
- Connector capabilities vs the eight connector crates (slack, gmail, calendar, whatsapp, telegram, gdrive, hackernews, linkedin)

Fix commands/features that are documented but gone, present but undocumented, or described incorrectly.

## Step 3: Final verification

The parallel work can conflict (shared `Cargo.toml` edits, refactors moving code that new tests import, removed deps used by new tests). Catch it all with:

```bash
./scripts/check.sh && cargo build --release
```

Fix anything red.

## Step 4: Combined report

Merge the three sub-agent reports plus the docs-sync findings into one summary, ending with a checklist of the final verification results and an **Action items** list for decisions only the user can make.

Do **not** commit — leave the review and commit to the user. If a sub-agent fails or stalls, note it in the report and continue with the others.
