---
id: TASK-255
title: 'Hotfix: restore missing view_system_vulnerabilities via migration'
status: Review
assignee: []
created_date: '2026-04-09 23:13'
updated_date: '2026-04-09 23:42'
labels:
  - hotfix
  - cve
  - database
  - migration
milestone: m-12
dependencies: []
references:
  - packages/default/src/handlers/api/dashboard.rs
  - packages/default/migrations/0024_update_views.sql
priority: high
ordinal: 2550
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem:
Production CVE dashboard endpoint `/api/v1/cves/summary` returns HTTP 500 because `public.view_system_vulnerabilities` is missing in some deployed databases. Builder CVE ingestion is working, but summary/drilldown queries depend on this view.

Desired Outcome:
Add a forward migration that (re)creates `public.view_system_vulnerabilities` with the current derivation_id-based schema so production upgrades are self-healing and do not require ad-hoc manual SQL.

Scope:
- Add idempotent migration only (no ad-hoc DB edits)
- Keep query/handler behavior unchanged
- Ensure migration works on already-correct DBs and on DBs missing the view
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Applying migrations creates `public.view_system_vulnerabilities` if missing
- [x] #2 Migration is idempotent and safe on databases where the view already exists
- [x] #3 `/api/v1/cves/summary` no longer fails due to missing relation after deploy
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added migration 0107_restore_view_system_vulnerabilities.sql to restore missing view via CREATE OR REPLACE.

Added regression test hotfix_migration_restores_view_system_vulnerabilities.

MR opened: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/222
<!-- SECTION:NOTES:END -->
