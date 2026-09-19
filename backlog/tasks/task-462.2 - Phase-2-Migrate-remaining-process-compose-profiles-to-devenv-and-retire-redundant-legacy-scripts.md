---
id: TASK-462.2
title: >-
  Phase 2: Migrate remaining process-compose profiles to devenv and retire
  redundant legacy scripts
status: To Do
assignee: []
created_date: '2026-09-19 14:52'
updated_date: '2026-09-19 14:53'
labels: []
milestone: m-1
dependencies:
  - TASK-462.1
references:
  - packages/devScripts/default.nix
  - packages/devScripts/db-usability-check.sh
  - .gitlab-ci.yml
  - docs/agents/database-safety.md
  - TESTING.md
  - docs/fixture-seeding.md
parent_task_id: TASK-462
priority: medium
type: enhancement
ordinal: 480000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Context

TASK-462.1 (Phase 1) proves that devenv processes with dynamic ports and Portless can replace fixed-port process-compose orchestration for the `run-ui-dev` workflow only. This task is the remaining migration once Phase 1 parity is proven and merged.

## Goal

Migrate every remaining `process-compose-flake` profile in `packages/devScripts/default.nix` (`server-only`, `server-stack-mock`, `oidc-stack`, `full-stack`, `cve-test`, `state-machine-test`, `dashboard-visibility-test`) to devenv processes using the pattern established in TASK-462.1, then remove or drastically simplify the orchestration devenv parity makes redundant.

## Scope (to be refined once Phase 1 lands)

- Port each remaining process-compose profile to devenv processes with dynamically allocated ports, preserving each profile's current readiness, ordering, and mock/dev-mode behavior (`AUTH_MODE`, `CRYSTAL_FORGE__SERVER__EXECUTION_MODE=mock`, etc.).
- Once every active workflow that depended on the fixed dev-database port (`3042`) has an isolated-port devenv equivalent, remove or simplify `packages/devScripts/db-usability-check.sh`'s worktree-ownership-detection workaround, since dynamic per-worktree ports remove the collision it exists to detect. Do not remove it while any workflow still depends on the fixed port.
- Remove the `process-compose-flake` / `services-flake` flake inputs and associated Nix code only if nothing in the repository still depends on them after migration; otherwise, document what remains and why.
- Update `docs/agents/database-safety.md`, `TESTING.md`, `docs/fixture-seeding.md`, and any other documentation that references fixed dev ports (`3042`/`3445`/`8080`) or the process-compose workflow, so they describe the current devenv-based workflow accurately.
- Update `.gitlab-ci.yml` targets that reference `devScripts.cve-test`, `devScripts.dashboard-visibility-test`, and `devScripts.state-machine-test` process-compose projects if their invocation changes.

## Non-goals

- Do not remove a legacy script or profile while any CI target, documented workflow, or active task still depends on it.
- Do not change the authoritative NixOS `web-ui` check or other `checks.*` outputs; those remain Nix-VM-based regardless of devshell orchestration changes.

## Acceptance criteria and verification plan are intentionally left for scoping once TASK-462.1 lands, so this task can reflect what Phase 1 actually proved rather than what was originally assumed. Do not move this task to `To Do` until that scoping pass is done.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every process-compose profile listed in scope has a devenv equivalent, or an explicit documented reason why it does not.
- [ ] #2 No CI target or documented developer workflow breaks as a result of removing or simplifying legacy scripts.
- [ ] #3 docs/agents/database-safety.md, TESTING.md, and docs/fixture-seeding.md accurately describe the current local-development workflow after migration.
- [ ] #4 db-usability-check.sh's worktree-ownership-detection workaround is removed or justified as still necessary, with the decision documented.
<!-- AC:END -->
