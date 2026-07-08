---
id: TASK-383
title: Fix enum casing mismatches in ui-screenshots routes.js fixture data
status: Backlog
assignee: []
created_date: '2026-07-05 02:43'
labels:
  - fixture-seeding
  - ui-screenshots
  - bug
dependencies: []
modified_files:
  - checks/ui-screenshots/routes.js
ordinal: 327000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The `ui-screenshots` fixture screenshots show API deserialization errors on several views because `checks/ui-screenshots/routes.js` emits enum values in the wrong case for the current DTOs.

Observed in fixture screenshots:
- Dashboard: "unknown variant `Building`, expected one of `idle`, `queued`, `building`, `cancelling`, `complete`, `failed`, `cancelled`" (build status casing)
- Systems: "unknown variant `Healthy`, expected one of `healthy`, `warning`, `critical`, `offline`" (health status casing)

`routes.js` was written to output PascalCase (`Healthy`, `Building`) but the DTOs now expect lowercase / snake_case variants. The DTO enums changed after routes.js was authored.

## Desired Outcome

Every fixture-driven view renders real data (no "API unavailable / Deserialization error" banners). Enum values emitted by routes.js match the current serde representations of the corresponding DTOs (health_status, build/job status, deployment_status, severity, etc.).

## Notes

- Cross-check each mapper helper in routes.js against the `#[serde(rename_all = ...)]` on the matching DTO in `packages/web-ui/src/api/models.rs`.
- This is the route-interception path; the DB-backed seed.rs path derives casing from the real handlers so it is not affected.
<!-- SECTION:DESCRIPTION:END -->
