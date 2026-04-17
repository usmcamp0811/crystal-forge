---
id: TASK-255
title: 'Hotfix: restore missing view_system_vulnerabilities via migration'
status: Done
assignee: []
created_date: '2026-04-09 23:13'
updated_date: '2026-04-14 00:34'
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
After CI rerun, builder/dashboard checks still failed in migration 107 due to removed schema columns (`pkg_d.package_name` then `d.status`).

Updated migration 0107 to current derivations schema and status model: package fields now sourced from `pkg_d.derivation_name/pname/version`, and nixos status filter now joins `derivation_statuses` via `d.status_id = ds.id` with `ds.name IN ('build-complete','complete')`.

Hardened migration regression test to assert modern column/filter usage and prevent reintroduction of removed `package_*`/`d.status` references.

Verification (offline): `SQLX_OFFLINE=true nix develop -c cargo test hotfix_migration_restores_view_system_vulnerabilities` and `SQLX_OFFLINE=true nix develop -c cargo test handle_duplicate_cleanup` both passed.

Pushed follow-up commits: df7cbee4 and 216b5155. New head pipeline started: https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2442813720
<!-- SECTION:NOTES:END -->
