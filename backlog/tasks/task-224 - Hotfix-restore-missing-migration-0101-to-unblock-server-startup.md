---
id: TASK-224
title: Hotfix restore missing migration 0101 to unblock server startup
status: To Do
assignee: []
created_date: '2026-03-29 14:35'
updated_date: '2026-03-29 14:35'
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
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 MR created and merged to dev as emergency hotfix.
- [ ] #2 Post-deploy confirmation captured: server starts without migration-101 error.
<!-- DOD:END -->
