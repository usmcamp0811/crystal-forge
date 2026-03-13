---
id: TASK-120
title: Restore dashboard SQL views dropped by migration cascade
status: Done
assignee: []
created_date: '2026-02-23 04:10'
updated_date: '2026-03-13 01:24'
labels: []
dependencies: []
ordinal: 74000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Running the dashboard summary endpoint can return HTTP 500 even with a running database because key views are missing after migrations. In a fresh dev DB with migrations up to 75, these views are absent: view_fleet_health_status, view_deployment_status, and view_systems_cve_summary.

Initial investigation shows migration 0070_update_view_nixos_pipeline_latest_with_deploy.sql includes DROP VIEW IF EXISTS view_buildable_derivations CASCADE; and recreates only view_buildable_derivations, which likely cascades and drops dependent views that are not recreated afterward.

## Desired Outcome
Ensure migrations leave dashboard-dependent views present and valid so GET /api/v1/dashboard/summary returns real data (or empty-state zeros) instead of 500 when DB is available.

Suggested acceptance checks:
- After running all migrations on a fresh DB, all dashboard views exist.
- SELECT against dashboard view set succeeds.
- GET /api/v1/dashboard/summary does not fail due to missing relations.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode on gray in /home/mcamp/code/crystal-forge/TASK-120-restore-dashboard-views

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/132

Verification: migration 0076 applied via cargo sqlx migrate run; dashboard views now exist and dashboard aggregate queries succeed; cargo test dashboard_summary passed.
<!-- SECTION:NOTES:END -->
