---
id: TASK-120
title: Restore dashboard SQL views dropped by migration cascade
status: Backlog
assignee: []
created_date: '2026-02-23 04:10'
labels: []
dependencies: []
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Running the dashboard summary endpoint can return HTTP 500 even with a running database because key views are missing after migrations. In a fresh dev DB with migrations up to 75, these views are absent: , , and .

Initial investigation shows migration  includes  and recreates only , which likely cascades and drops dependent views that are not recreated afterward.

## Desired Outcome
Ensure migrations leave dashboard-dependent views present and valid so  returns real data (or empty-state zeros) instead of 500 when DB is available.

Suggested acceptance checks:
- After running all migrations on a fresh DB, all dashboard views exist.
-  against dashboard view set succeeds.
-  does not fail due to missing relations.
<!-- SECTION:DESCRIPTION:END -->
