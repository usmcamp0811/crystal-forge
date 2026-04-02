---
id: TASK-232
title: >-
  Restore missing migration 0102 on dev to resolve startup failure against
  applied DB version
status: Done
assignee: []
created_date: '2026-03-31 12:34'
updated_date: '2026-04-02 00:08'
labels:
  - hotfix
  - database
  - migration
  - outage
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Production/startup fails with `migration 102 was previously applied but is missing in the resolved migrations`. The `dev` branch currently does not include migration file `0102_add_expected_store_path_to_derivations.sql`, but target databases already have version 102 recorded in `_sqlx_migrations`.

## Desired Outcome
`dev` and deployable artifacts include migration 0102 exactly as originally applied so server startup/migration resolution succeeds without altering production DB data.

## Scope
- Reintroduce migration `packages/default/migrations/0102_add_expected_store_path_to_derivations.sql` from the known applied commit content.
- Validate DB check target that exercises migrations.
- Open emergency MR for fast review/merge.

## Non-Goals
- No schema redesign.
- No direct production DB mutation.
- No unrelated code changes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `packages/default/migrations/0102_add_expected_store_path_to_derivations.sql` exists on the branch with expected content.
- [x] #2 `nix build .#checks.x86_64-linux.database --no-link` passes locally.
- [x] #3 Emergency MR to `dev` is opened with outage context and verification evidence.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Moved Backlog -> To Do per explicit user request to repair outage.

LOCK: opencode-gpt5 on reckless in /home/mcamp/code/crystal-forge/TASK-232-restore-migration-0102

Emergency hotfix commit: `5b02291f` restoring `packages/default/migrations/0102_add_expected_store_path_to_derivations.sql`.

MR opened: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/202

Verification (non-production): `nix build .#checks.x86_64-linux.database --no-link` PASS.

Backlog review sync: branch head `c0791cf9` is now contained in `origin/dev` (merged). Worktree cleanup completed (`~/code/crystal-forge/TASK-232-restore-migration-0102` removed, `git worktree prune` run).
<!-- SECTION:NOTES:END -->
