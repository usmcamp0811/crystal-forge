---
id: TASK-236
title: Fix local test DB auth for cf_test to allow non-TLS SQLx integration tests
status: Backlog
assignee: []
created_date: '2026-04-01 03:41'
labels:
  - testing
  - database
  - dev-environment
dependencies: []
references:
  - packages/default/src/test_utils/db.rs
  - packages/default/src/queries/builders.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: DB-backed ignored tests that use `postgres://postgres:postgres@localhost/cf_test` (and 127.0.0.1 variant) fail in current dev environment with `pg_hba.conf rejects connection ... no encryption`, while SQLx in this package is built without TLS support for those test paths.

Desired Outcome: Align local/dev test database configuration and test connection defaults so DB-backed integration tests for `cf_test` run successfully under `nix develop` without requiring TLS configuration hacks.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Document and standardize supported local test DB connection settings for DB-backed tests.
- [ ] #2 Running ignored DB-backed tests against `cf_test` succeeds under repository dev workflow.
- [ ] #3 No production DB credentials or unsafe defaults are introduced.
<!-- AC:END -->
