---
id: TASK-462.1
title: >-
  Phase 1: Add a devenv shell with dynamic-port processes and prove
  parallel-worktree isolation for run-ui-dev
status: To Do
assignee: []
created_date: '2026-09-19 14:51'
labels: []
milestone: m-1
dependencies: []
references:
  - shells/default/default.nix
  - packages/devScripts/default.nix
  - packages/devScripts/db-only-start.sh
  - packages/devScripts/db-usability-check.sh
  - checks/web-ui/tests/web-ui-test.sh
  - docs/agents/worktrees.md
  - docs/agents/database-safety.md
  - docs/agents/verification.md
parent_task_id: TASK-462
priority: high
type: enhancement
ordinal: 479000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Context

Read TASK-462 for the full problem statement, scope boundary, and constraints. This subtask is Phase 1: the minimum change that proves the devenv migration is viable, without touching the existing `nix develop` shell or any `process-compose-flake` profile other than by addition.

Today, `run-ui-dev` (`packages/devScripts/default.nix`) starts PostgreSQL on the fixed port `3042` via the `db-only` process-compose profile, waits for `pg_isready`, then runs `db-usability-check.sh` to detect whether the process answering on that port actually belongs to this worktree (see the extensive comment above that call site explaining why this check exists: every worktree shares the fixed port `3042` but has its own on-disk data directory, so a different worktree's leftover PostgreSQL process, or a database owned by an unrelated role, can answer instead). The API server binds `3445` and the Dioxus dev server binds `8080`, both fixed. `checks/web-ui/tests/web-ui-test.sh` defaults to those same two fixed addresses via `CF_UI_DEV_BASE_URL` / `CF_UI_DEV_API_BASE_URL`, which it already accepts as environment overrides.

## Goal

Add a devenv-based development shell, additive to the existing `nix develop` shell, that reproduces the `run-ui-dev` workflow (PostgreSQL + Crystal Forge API server + Dioxus web UI dev server, with fixture seeding and dev-key generation) using devenv processes with dynamically allocated, per-worktree ports. Prove that two worktrees can run this workflow at the same time with zero manual port coordination and zero risk of one worktree reusing another's database or server process. Layer Portless stable `.localhost` URLs on top as an opt-in convenience.

## In scope

1. Add `devenv` as a flake input and a new `devShells` output (for example `devShells.devenv`) built with `devenv.lib.mkShell`, alongside the existing `devShells.default`. Do not modify or remove `shells/default/default.nix` or any existing alias/script it defines.
2. Model PostgreSQL, the Crystal Forge API server, and the Dioxus web UI dev server as devenv `processes`, reproducing what `runUiDev` currently does by hand: readiness waiting, dev-key generation, fixture seeding, and Tailwind/wasm-bindgen setup for the UI process.
3. Give the API server and web UI dev server processes dynamically allocated ports via devenv's automatic port allocation (`ports.<name>.allocate`). Pass the API server's resolved port to the web UI process through its environment so the UI can reach its own worktree's API, not another worktree's.
4. Give PostgreSQL an isolated, non-fixed-port identity per worktree. Investigate and document the chosen mechanism (an allocated TCP port, or a per-worktree Unix socket) as part of this task; do not hardcode a derived port such as `3042 + hash(worktree)`.
5. Derive the devenv project identity (the value that seeds Portless `.localhost` hostnames) from a value that is stable per worktree and distinct across worktrees, such as the worktree's directory basename (the `TASK-ID-short-slug` convention from `docs/agents/worktrees.md`). Do not let two worktrees share the same devenv project identity.
6. Enable Portless (`process.proxy.enable`) for the API server and web UI processes so they get stable `http://<name>.<worktree-identity>.localhost` URLs. Portless stays opt-in: every acceptance criterion below must also pass with Portless disabled, and nothing in this workflow may require the port-80 proxy or its sudo prompt to function.
7. Export the resolved API and UI base URLs through the existing `CF_UI_DEV_BASE_URL` and `CF_UI_DEV_API_BASE_URL` environment variables so `checks/web-ui/tests/web-ui-test.sh` runs against the new stack without modification.
8. Prove parallel-worktree isolation: start the new devenv workflow in two separate worktrees at the same time, confirm each has its own PostgreSQL, API server, and UI process bound to independent ports (and, if Portless is enabled, independent hostnames), and run `web-ui-test` independently and successfully in both without stopping, restarting, or otherwise touching the other worktree's processes.
9. Add developer-facing documentation (README, onboarding doc, or `docs/agents/*`, matching existing documentation conventions) explaining: how to start the new devenv workflow, how to find the resolved ports/hostnames for a given worktree, how Portless is enabled/disabled, and how this workflow relates to (and does not yet replace) the existing `nix develop` / `run-ui-dev` / process-compose workflow.
10. Update `docs/agents/database-safety.md` if the new workflow changes what "isolated local development database" means or how to verify it; otherwise, state explicitly in the task notes why no update is needed.

## Out of scope (tracked separately or in the parent task)

- Migrating `server-only`, `server-stack-mock`, `oidc-stack`, `full-stack`, `cve-test`, `state-machine-test`, or `dashboard-visibility-test` to devenv.
- Removing `process-compose-flake`, `services-flake`, `db-only-start.sh`, `db-usability-check.sh`, or any existing devShell alias. All existing workflows must keep working exactly as before.
- Changing the authoritative NixOS `web-ui` check or any other `checks.*` output.
- Sharing a Cargo `target/` directory or compiler cache across worktrees (TASK-450.6).
- HTTPS via Portless/mkcert, Linux capability grants, or any devenv feature not needed to satisfy the acceptance criteria above.

## Risk

Portless requests sudo authentication for its port-80 listener on Linux. This must never block or fail the base (non-Portless) workflow, and must not be silently required by any acceptance criterion or by `web-ui-test`.

## Verification plan

- `nix flake check --keep-going`: confirms the new flake input/output does not break existing flake evaluation or any hermetic check.
- Manually enter the existing `nix develop` shell and confirm `run-ui-dev`, `db-only`, and other existing aliases still behave exactly as before (no regression from adding the new shell).
- Enter the new devenv shell in two separate worktrees at the same time; record the exact commands run and the resolved ports (and hostnames, if Portless is enabled) observed in each worktree as verification evidence.
- Run `web-ui-test` in both worktrees concurrently against their own stacks and record that both pass independently.
- Repeat the two-worktree proof with Portless disabled to confirm it is not required.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A new devenv-based `devShells` output exists alongside the existing `devShells.default`; entering the existing `nix develop` shell and its aliases (`run-ui-dev`, `db-only`, etc.) is unaffected.
- [ ] #2 PostgreSQL, the Crystal Forge API server, and the Dioxus web UI dev server run as devenv processes that reproduce the current `run-ui-dev` behavior (readiness waiting, dev-key generation, fixture seeding, Tailwind/wasm-bindgen setup).
- [ ] #3 The API server and web UI dev server use devenv's automatic port allocation instead of the fixed ports 3445 and 8080; the UI process receives its own worktree's resolved API port through its environment.
- [ ] #4 PostgreSQL runs with a per-worktree isolated port or socket instead of the fixed port 3042, with the chosen mechanism documented and justified.
- [ ] #5 The devenv project identity used for Portless hostnames is derived from a per-worktree-unique value (such as the worktree directory basename) so two worktrees never generate colliding `.localhost` hostnames.
- [ ] #6 Portless (`process.proxy.enable`) is wired for the API and web UI processes and produces stable `.localhost` URLs, and every other acceptance criterion in this task also passes with Portless disabled.
- [ ] #7 `checks/web-ui/tests/web-ui-test.sh` runs unmodified against the new stack by consuming resolved ports through the existing `CF_UI_DEV_BASE_URL` / `CF_UI_DEV_API_BASE_URL` overrides.
- [ ] #8 Two worktrees run the new devenv workflow at the same time, each with its own isolated PostgreSQL/API/UI processes and ports, and `web-ui-test` passes independently in both; the exact commands and observed ports/hostnames are recorded as verification evidence.
- [ ] #9 Developer-facing documentation explains how to start the new devenv workflow, inspect resolved ports/hostnames, enable/disable Portless, and how the new workflow relates to the existing `nix develop`/process-compose workflow it does not yet replace.
- [ ] #10 `nix flake check --keep-going` passes, and no existing package, NixOS module, or `checks.*` output changes behavior as a result of this task.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Two-worktree parallel isolation proof recorded with exact commands and resolved ports/hostnames for each worktree
- [ ] #2 nix flake check --keep-going passes
- [ ] #3 Existing nix develop shell and process-compose scripts verified unmodified in behavior after this change
<!-- DOD:END -->
