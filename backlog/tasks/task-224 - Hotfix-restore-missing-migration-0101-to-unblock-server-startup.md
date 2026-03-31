---
id: TASK-224
title: Hotfix restore missing migration 0101 to unblock server startup
status: Done
assignee: []
created_date: '2026-03-29 14:35'
updated_date: '2026-03-31 01:57'
labels:
  - hotfix
  - database
  - migration
  - production
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

Production server fails to start after deploy with:

`migration 101 was previously applied but is missing in the resolved migrations`

The database has migration version 101 recorded in `_sqlx_migrations`, but current code resolves migrations only through `0100`, causing startup abort/restart loop.

## Goal

Restore the missing migration file `0101` exactly as originally applied so SQLx migration resolution matches production state and server boots successfully.

## Non-Goals

- No schema redesign.
- No new migration logic beyond restoring the missing historical migration.
- No changes to unrelated application code.

## Architectural Constraints

- Migration content must match original applied file semantics.
- Preserve migration ordering and naming convention.
- Keep scope limited to migration restoration.

## Verification Plan

- Confirm file `packages/default/migrations/0101_add_flake_credentials_build_scope_and_system_config.sql` exists in branch.
- Run targeted compile check in repo dev environment.
- Validate MR contains only migration restoration (and task bookkeeping if applicable).

## Impact Areas

- `packages/default/migrations/0101_add_flake_credentials_build_scope_and_system_config.sql`

## Risk Level

High (production outage until fixed).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Restored migration file `0101_add_flake_credentials_build_scope_and_system_config.sql` is present in migrations directory.
- [ ] #2 Migration ordering resolves through 0101 in deployed build, eliminating startup error about missing migration 101.
- [ ] #3 No unrelated files are changed by the hotfix.
- [ ] #4 Server startup no longer fails with SQLx migration mismatch after deploy.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Promoted to To Do by maintainer emergency request to unblock production startup.

LOCK: opencode-gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-224-restore-migration-0101

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/195

Restored migration file from historical commit 098fe16df4ef4026a149ec56830f160e189863e5.

Verification note: `nix develop -c cargo check` in packages/default failed in this environment due SQLx DB connection refused (os error 111), unrelated to migration-file restore scope.

Root-cause context captured: migration `0101` was introduced on `TASK-221-flake-credentials-modal-ui` (commit `098fe16df4ef4026a149ec56830f160e189863e5`) and had already been applied in production DB. Later deploys from `dev` lacked the file, causing SQLx startup failure (`migration 101 ... missing in resolved migrations`).

Hotfix strategy intentionally restores the exact historical `0101` file content instead of modifying `_sqlx_migrations` DB records, to preserve migration-chain integrity and prevent checksum/order drift.

Conflict resolution note: original TASK-224 branch had backlog-history conflict noise during rebase; clean branch was rebuilt from `origin/dev` and hotfix commit was cherry-picked, resulting in commit `1d7ed9a7` on MR195.
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 MR created and merged to dev as emergency hotfix.
- [ ] #2 Post-deploy confirmation captured: server starts without migration-101 error.
<!-- DOD:END -->
