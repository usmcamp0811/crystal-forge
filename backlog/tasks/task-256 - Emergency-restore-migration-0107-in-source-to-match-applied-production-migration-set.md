---
id: TASK-256
title: >-
  Emergency: restore migration 0107 in source to match applied production
  migration set
status: Backlog
assignee: []
created_date: '2026-04-10 01:07'
labels:
  - hotfix
  - database
  - migration
  - production
milestone: m-12
dependencies: []
references:
  - packages/default/migrations
  - packages/default/src/queries/systems.rs
priority: high
ordinal: 2560
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem Statement:
Production server restart loops on startup with `migration 107 was previously applied but is missing in the resolved migrations`. The production database has migration version 107 recorded in `_sqlx_migrations`, but the deployed source revision no longer resolves migration 0107, causing sqlx migrator startup failure.

Goal:
Ship an emergency patch that restores migration 0107 in source control so server startup migration resolution is consistent with already-applied production state.

Non-Goals:
- No schema redesign beyond restoring migration presence/compatibility.
- No unrelated refactors.
- No changes outside migration/test and minimal task metadata.

Verification Plan:
- Verify migration file 0107 exists in `packages/default/migrations`.
- Run targeted migration-regression test(s) in nix develop.
- Run targeted server/package checks proving migration set resolves at runtime in CI.

Architectural Constraints:
- Keep migration history append-only and compatible with sqlx migration tracking.
- Avoid changing unrelated migration versions.

Impact Areas:
- packages/default/migrations
- migration regression tests in packages/default/src/queries/systems.rs
- CI builder/dashboard checks relying on startup migrations

Risk Level:
High (production outage due to server crash loop).

Dependencies:
None.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Server startup no longer errors with "migration 107 was previously applied but is missing in the resolved migrations"
- [ ] #2 Migration 0107 is present in source and compatible with existing migration chain
- [ ] #3 Targeted migration-related tests pass in nix develop
- [ ] #4 MR is opened with verification results
<!-- AC:END -->
