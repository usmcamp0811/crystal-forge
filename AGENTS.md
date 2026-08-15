# Crystal Forge Agent Guide

This file governs work performed by automated agents in this repository. Follow higher-priority platform and safety instructions first. When this file, an active task, and a user request disagree, stop before making changes and ask the user to resolve any conflict that materially affects scope or behavior.

## Start by classifying the request

| Request                         | Backlog task                                      | Dedicated worktree | Repository writes                       |
| ------------------------------- | ------------------------------------------------- | ------------------ | --------------------------------------- |
| Explain, answer, or explore     | Not required                                      | Not required       | No                                      |
| Review or diagnose              | Not required                                      | Not required       | No, unless the user also asks for a fix |
| Maintain or groom the backlog   | No implementation task required                   | Not required       | Backlog-related files only              |
| Implement a change              | Required and `To Do`                              | Required           | Active-task scope only                  |
| Work on the next available task | Select the highest-priority eligible `To Do` task | Required           | Active-task scope only                  |

Do not change files, backlog state, branches, or merge requests for a read-only request. If a request changes from analysis to implementation, perform the implementation preflight before writing.

## Core rules

1. Follow the user's requested task. Do not substitute a different task merely because it has a higher backlog priority. Select the highest-priority task only when the user explicitly asks for the next available work.
2. Do not modify application code without an active, sprint-ready backlog task in `To Do`.
3. Use one dedicated branch and worktree per implementation task. Never implement in the `main` or `dev` integration worktree.
4. Preserve user changes. Never discard, overwrite, or reformat unrelated work.
5. Keep changes within the active task's acceptance criteria. Record unrelated discoveries as new `Backlog` tasks; do not implement them unless the user expands the scope.
6. Follow existing repository patterns before introducing new abstractions.
7. Run verification appropriate to the affected behavior. Never claim a command passed unless it was run and its actual exit status was successful.
8. Use the repository's Nix development environment for project toolchains and verification.
9. Do not merge an MR unless the user explicitly authorizes it.
10. Ask before making a decision that materially changes public behavior, compatibility, persistence, security boundaries, architecture, or task scope.
11. Don't rely on utilities to be installed like python or glab.. just use `nix run nixpkgs#glab` for these type of things

## Repository architecture

Crystal Forge is a Nix flake containing Rust server, agent, and builder components, a Dioxus WASM frontend, PostgreSQL persistence through SQLx, and Nix/Playwright integration checks.

Preserve these boundaries:

- The server owns persistence, authorization, job coordination, and server-side domain policy.
- API-only builders do not access the Crystal Forge database directly.
- Builder sessions and server-issued job authorization must remain enforced at API boundaries.
- UI views compose presentation and interaction. Put reusable state transitions and domain decisions outside view markup.
- Browser/WASM code must use browser-compatible APIs. Do not assume native `std::time`, filesystem, process, or socket behavior is available in WASM.
- Database schema changes require migrations. SQLx compile-time metadata must match schema and query shapes.
- Maintain compatibility with supported deployed agents/builders unless the active task explicitly defines a breaking transition.
- Treat bootstrap signing, session validation, cache verification, derivation transport, secret redaction, and authorization checks as security-sensitive code.
- Design documents named by an active task are authoritative for that task. Existing behavior and tests are evidence, but do not silently override an explicit design requirement.

Avoid arbitrary abstractions. Introduce a trait, shared component, or new layer only when it expresses a real boundary, enables needed testing, or matches an established repository pattern. Do not refactor solely to satisfy a line-count target.

## Backlog workflow

Use the Backlog.md MCP integration when available; otherwise use the repository-provided Backlog CLI from `nix develop`. See [docs/agent/backlog-workflow.md](docs/agent/backlog-workflow.md).

The valid lifecycle is:

```text
Backlog -> To Do -> In Progress -> Review -> Done
```

- New discoveries default to `Backlog`.
- Only a human selects work for a sprint by moving `Backlog` to `To Do`, unless the user explicitly delegates that decision.
- `In Progress` requires a task lock and dedicated worktree.
- `Review` requires an open MR and completed verification. During this time the MR will be deployed to a test server and any changes requested must be done and will result in new database migrations NOT! edits to existing mirations.
- `Done` requires the MR to be merged and the task worktree to be removed.

## Implementation preflight

Before the first implementation write:

1. Resolve the user-requested task and read its acceptance criteria, non-goals, dependencies, risk, and verification plan.
2. Confirm it is sprint-ready, in `To Do`, and has no active lock.
3. Discover the integration worktrees and verify `main` and `dev` are clean.
4. Create a task branch and worktree from the designated integration branch, normally `dev`.
5. Verify the new worktree, branch, base, and status.
6. Move the task to `In Progress` and add its lock.
7. State a concise preflight containing the task, worktree, branch/base, intended scope, and verification plan.

Do not pretend that `git status` in one worktree proves another worktree is clean. Exact commands and recovery rules are in [docs/agent/worktrees.md](docs/agent/worktrees.md).

## Implementation standards

### Rust

- Use `Result`-based error handling and preserve useful error context.
- Do not use `unwrap` or `expect` on reachable production error paths.
- Avoid unnecessary cloning and blocking work in async execution paths.
- Keep API models, domain decisions, persistence, and transport concerns separated according to existing modules.
- Use the SQLx query form that best matches the query and surrounding repository conventions.

### Dioxus/WASM

- Keep rendering code focused on presentation and event wiring.
- Extract nontrivial state transitions and test them independently when practical.
- Keep client DTOs aligned with the server contract, but do not duplicate server types when the UI intentionally needs a different representation.
- Preserve loading, empty, error, authorization, and stale-data behavior when changing views.
- A user-visible UI change must be exercised by the authoritative `web-ui` check and represented by an MR screenshot. Add a behavioral assertion when practical.

### Database and SQLx

- Add a migration for every schema change; never edit an already-applied migration unless repository policy explicitly permits it.
- Update SQLx offline metadata for changes to migrations, checked queries, selected columns, bind parameters, or query result shapes.
- Perform destructive database reset/refresh operations only against a verified isolated local development database started by this repository.
- Never use a shared, staging, production, or default local PostgreSQL instance for SQLx preparation.

See [docs/agent/database-safety.md](docs/agent/database-safety.md).

## Verification

Choose the smallest set of commands that proves the acceptance criteria and protects affected interfaces. Prefer targeted checks during implementation and broader checks before review when risk warrants them.

Use the exact package manifests and flake attributes applicable to the change. The baseline command matrix is in [docs/agent/verification.md](docs/agent/verification.md).

Run `nix flake check --keep-going` when the task requires it or the change affects flakes, NixOS modules, development shells, packaging, CI/release behavior, or cross-package interfaces that targeted checks cannot adequately prove. Do not run it mechanically for every edit.

If a required check cannot run, report the command, failure, and impact. Do not move the task to `Review` while required verification is incomplete.

Output summarizers such as `distill` are optional reading aids, never proof. Preserve and inspect the underlying command's real exit status. Do not summarize output that must be retained verbatim or parsed mechanically.

## Scope and safety

- Inspect freely within the repository when needed to understand the task.
- Make reasonable, reversible, repository-consistent assumptions when they do not change scope or public behavior.
- Do not delete files, rename public modules, add dependencies, change CI, or introduce breaking API/schema behavior unless the task requires it.
- Do not expose secrets, authorization headers, credentials, signed URLs, or sensitive environment data in logs, tests, task notes, commits, or MR descriptions.
- Do not use destructive Git commands to resolve an unexpected dirty worktree.
- If verification reveals an unrelated defect, create a Backlog task and continue only if the active task remains safely verifiable.

Stop and ask the user when acceptance criteria conflict, a required dependency is unavailable, the correct migration or compatibility strategy is ambiguous, a destructive operation cannot be proven local and isolated, or continuing would overwrite someone else's work.

## Review and completion

Before opening an MR:

- Confirm every acceptance criterion.
- Run the declared verification and record exact commands and outcomes.
- Confirm only intended files changed and all intended new files are tracked.
- Update SQLx metadata when applicable.
- Add MR screenshots for user-visible UI changes.
- Record out-of-scope discoveries as Backlog tasks.

Then open the MR, move the task to `Review`, add the MR link to the task, and remove the lock or mark it as awaiting review according to Backlog.md conventions.

After the MR is merged, remove the task worktree, prune stale worktree metadata if necessary, and move the task to `Done`. Do not report the task as complete merely because implementation was pushed.

## Reporting

Be precise and concise. Distinguish among:

- implemented but not verified;
- verified locally;
- pushed to a branch;
- open for review;
- merged and complete.

Never fabricate command output, test results, task state, commits, pushes, MR state, or screenshots.
