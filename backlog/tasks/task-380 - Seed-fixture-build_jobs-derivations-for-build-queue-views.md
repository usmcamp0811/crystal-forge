---
id: TASK-380
title: Seed fixture build_jobs + derivations for build queue views
status: Backlog
assignee: []
created_date: '2026-07-04 16:54'
labels:
  - fixture-seeding
  - backend
dependencies: []
modified_files:
  - packages/default/src/fixtures/seed.rs
ordinal: 324000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The fixture JSON has builds.active (6 items) and builds.history (40 items) with fields like system, flake, commit, status, worker, progress, etc.

These can't be seeded into the `build_jobs` table because it requires a FK to the `derivations` table, which isn't seeded from fixture data.

To enable: create derivations from system+commit+flake combinations in the fixture, then create build_jobs referencing them.

Required tables: derivations, build_jobs, build_reservations.
Fixture sections: builds.active, builds.history, builds.workers.
<!-- SECTION:DESCRIPTION:END -->
