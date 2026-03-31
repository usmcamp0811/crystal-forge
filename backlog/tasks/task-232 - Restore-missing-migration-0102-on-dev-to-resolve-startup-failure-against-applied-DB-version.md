---
id: TASK-232
title: >-
  Restore missing migration 0102 on dev to resolve startup failure against
  applied DB version
status: Backlog
assignee: []
created_date: '2026-03-31 12:34'
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
- [ ] #1 `packages/default/migrations/0102_add_expected_store_path_to_derivations.sql` exists on the branch with expected content.
- [ ] #2 `nix build .#checks.x86_64-linux.database --no-link` passes locally.
- [ ] #3 Emergency MR to `dev` is opened with outage context and verification evidence.
<!-- AC:END -->
