---
id: TASK-382
title: 'Seed fixture compliance, cache destinations, scanning, and admin audit data'
status: Backlog
assignee: []
created_date: '2026-07-04 16:54'
labels:
  - fixture-seeding
  - backend
dependencies: []
modified_files:
  - packages/default/src/fixtures/seed.rs
ordinal: 326000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Several fixture JSON sections are not seeded into the database because their tables were deemed optional for the initial integration:

1. compliance - 4 bundles with policy IDs and environment requirements
   -> Tables: compliance_bundles, compliance_bundle_policies, compliance_bundle_environments

2. caches - 5 cache destinations with S3/Atatic configs, environment assignments
   -> Tables: cache_destinations, cache_destination_environments, cache_push_jobs

3. scanning - config, stats, activity, history
   -> Tables: scan_schedule_policy, cve_scans, scan_packages

4. admin.auditLog - admin audit event entries
   -> Table: admin_audit_events

5. admin.oidcMappings - OIDC group mappings
   -> Table: oidc_group_mappings

These are relatively straightforward INSERTs from fixture data.
<!-- SECTION:DESCRIPTION:END -->
