---
id: TASK-462
title: >-
  Adopt devenv process orchestration with Portless URLs for parallel-worktree
  local development
status: Backlog
assignee: []
created_date: '2026-09-19 14:50'
updated_date: '2026-09-19 14:50'
labels: []
milestone: m-1
dependencies: []
references:
  - shells/default/default.nix
  - packages/devScripts/default.nix
  - packages/devScripts/db-usability-check.sh
  - packages/devScripts/db-usability-check-test.sh
  - checks/web-ui/tests/web-ui-test.sh
  - docs/agents/worktrees.md
  - docs/agents/database-safety.md
  - 'https://devenv.sh/processes/'
  - 'https://devenv.sh/blog/2026/09/07/devenv-23-portless-and-tui-configuration/'
  - 'https://devenv.sh/guides/using-with-flakes/'
  - 'https://devenv.sh/services/postgres/'
priority: medium
type: enhancement
ordinal: 478000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

This repository runs one dedicated Git worktree per task (`docs/agents/worktrees.md`). Several task worktrees exist at the same time, but the local development stack uses fixed ports:

- PostgreSQL: `127.0.0.1:3042` (`shells/default/default.nix`, `packages/devScripts/default.nix`)
- Crystal Forge API server: `127.0.0.1:3445`
- Dioxus web UI dev server: `127.0.0.1:8080`

Every worktree's `db-only`/`run-ui-dev` PostgreSQL instance binds the same port with its own on-disk data directory. When a second worktree starts its stack, it either fails to bind the port or, worse, silently reuses a different worktree's already-running PostgreSQL process. `packages/devScripts/db-usability-check.sh` exists specifically to detect this: it checks OS-level process ownership of the listening socket against the calling worktree's expected data directory before `run-ui-dev` is allowed to proceed. `db-usability-check-test.sh` documents the scenarios this guards against.

This means an agent working in one worktree can be blocked, or can accidentally target another worktree's database or server, purely because of a fixed-port collision. The `web-ui-test` harness has the same shape of problem: it defaults to `http://127.0.0.1:8080` and `http://127.0.0.1:3445` (`checks/web-ui/tests/web-ui-test.sh`), so two worktrees running it at the same time collide unless a developer manually coordinates ports.

## Goal

Let each worktree run its own isolated development stack (PostgreSQL, API server, web UI dev server) at the same time as every other worktree, with no manual port coordination, no shared database state, and no risk of one worktree's stack silently reusing another worktree's process.

[devenv](https://devenv.sh) 2.0+ provides this directly for the process-orchestration layer:

- **Automatic port allocation** (`processes.<name>.ports.<port>.allocate`): devenv finds a free port starting from a requested base and exposes the resolved value at `config.processes.<name>.ports.<port>.value` for other processes to consume declaratively. See <https://devenv.sh/processes/#automatic-port-allocation>.
- **Portless** (`process.proxy.enable`, devenv 2.3+): gives HTTP processes stable `http://<port-name>.<project-name>.localhost` URLs regardless of which physical port was allocated. See <https://devenv.sh/processes/#friendly-localhost-urls>. On Linux the shared proxy needs sudo to bind port 80; it must stay optional, not a requirement for the resolved-port workflow.
- **Flake integration** (`devenv.lib.mkShell` as an additional `devShells.<name>` output): devenv can be added to an existing flake without replacing it. See <https://devenv.sh/guides/using-with-flakes/>.

## Scope boundary: replace orchestration, not packaging

Keep unchanged:

- `flake.nix` outputs for packages, NixOS modules, and `checks.*` used for hermetic, reproducible verification (the authoritative `web-ui` NixOS check, server/agent/builder package builds, etc.).
- Production packaging and deployment behavior.

Replace, incrementally and only after parity is proven, the development-time orchestration currently split across:

- `mkShell` in `shells/default/default.nix`
- `process-compose-flake` + `services-flake` profiles in `packages/devScripts/default.nix` (`dbOnly`, `full-stack`, `server-only`, `server-stack-mock`, `oidc-stack`, `cveTest`, `stateMachineTest`, `dashboardVisibilityTest`)
- Hand-written fixed-port plumbing and worktree-ownership detection (`db-usability-check.sh`, the port-collision comments in `runUiDev`)

## Constraints

- Do not remove or weaken `db-usability-check.sh` protections until the devenv-based path fully replaces the workflow it guards; both can coexist during migration.
- PostgreSQL must not be routed through the Portless HTTP proxy; Portless is HTTP-only. Investigate whether `services.postgres.port` should be paired with devenv's own allocation mechanism or a per-worktree Unix socket; do not hardcode a derived port such as `3042 + hash(worktree)`.
- Every worktree's devenv project identity (the value that seeds the `.localhost` hostname) must be distinct per worktree. Do not let every checkout use the same literal project name; derive it from the worktree's directory basename (the `TASK-ID-short-slug` convention from `docs/agents/worktrees.md`) or another value that is stable per worktree and unique across worktrees.
- Portless must remain optional. CI, the authoritative NixOS `web-ui` check, and any sudo-less automated agent path must keep working from plain allocated ports; nothing may require the port-80 proxy to function.
- `checks/web-ui/tests/web-ui-test.sh` already reads `CF_UI_DEV_BASE_URL` / `CF_UI_DEV_API_BASE_URL` as overrides. Prefer exporting resolved devenv values through these existing variables over changing the test harness.
- This is a developer/agent inner-loop change. It must not alter hermetic Nix package or check build behavior, derivation hashes, or CI pipelines that do not opt into the new shell.

## Proposed phased plan

1. Add a devenv-based `devShells` output alongside the existing `nix develop` shell (additive, non-breaking) and port the `run-ui-dev` workflow (PostgreSQL + API + Dioxus dev server) onto devenv processes with dynamically allocated ports. *(Phase 1 — see the linked sprint-ready subtask.)*
2. Prove two worktrees can run the new workflow at the same time: each gets its own PostgreSQL, API, and UI process on independent ports/hostnames, and `web-ui-test` passes independently in both. *(Phase 1.)*
3. Enable Portless stable `.localhost` URLs as an opt-in convenience. *(Phase 1.)*
4. Migrate the remaining `process-compose-flake` profiles (`server-only`, `server-stack-mock`, `oidc-stack`, `full-stack`, `cve-test`, `state-machine-test`, `dashboard-visibility-test`) to devenv processes. *(Phase 2 — separate task.)*
5. Remove or drastically simplify the custom orchestration that devenv parity has made redundant (`db-usability-check.sh`'s ownership-detection workaround, fixed-port plumbing, redundant process-compose profiles) only after parity is proven and the old scripts are no longer needed by any active workflow. *(Phase 2 — separate task.)*

Do not attempt a wholesale conversion in one change. Each phase must independently prove parity before the previous mechanism is touched.

## Related work

- TASK-450.6 addresses sharing Rust compilation across worktrees via a compiler cache; it is a separate, complementary inner-loop improvement and is not a dependency of this task.
- `docs/agents/database-safety.md` and `docs/agents/worktrees.md` describe the current process-compose-based database safety workflow and worktree conventions; both need review once a devenv-based path exists so they describe the current recommended workflow accurately.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Phase 1 (the linked sprint-ready subtask) is complete: a devenv-based development shell exists, models PostgreSQL + API server + web UI dev server as processes with dynamically allocated ports, and is proven to run correctly in two worktrees at the same time.
- [ ] #2 Portless stable `.localhost` URLs work for the web UI and API processes as an opt-in convenience, and every workflow this task touches still works with Portless disabled.
- [ ] #3 A follow-up task tracks migrating the remaining process-compose profiles and retiring redundant legacy scripts once devenv parity is proven for them.
- [ ] #4 `docs/agents/database-safety.md` and `docs/agents/worktrees.md` (or their replacements) accurately describe whichever local-database workflow is current after this epic's phases land.
<!-- AC:END -->
