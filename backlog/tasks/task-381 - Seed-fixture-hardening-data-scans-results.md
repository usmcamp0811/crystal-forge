---
id: TASK-381
title: Seed fixture hardening data (scans + results)
status: Backlog
assignee: []
created_date: '2026-07-04 16:54'
labels:
  - fixture-seeding
  - backend
dependencies: []
modified_files:
  - packages/default/src/fixtures/seed.rs
ordinal: 325000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The fixture JSON has hardening (30 items) with service-level hardening scores, missing directives, risk levels, etc.

These need to be seeded into:
- hardening_scans (requires FK to derivations)
- service_hardening_results (requires FK to hardening_scans)

Currently skipped because both require derivations FK. Create synthetic derivations first, then populate hardening data linking to system states via derivation hostnames.

Required tables: hardening_scans, service_hardening_results, hardening_justifications.
Fixture sections: hardening.
<!-- SECTION:DESCRIPTION:END -->
