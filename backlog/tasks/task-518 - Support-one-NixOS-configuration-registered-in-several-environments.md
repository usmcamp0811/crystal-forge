---
id: TASK-518
title: Support one NixOS configuration registered in several environments
status: Backlog
assignee: []
created_date: '2026-11-04T03:33:00Z'
labels: []
dependencies: []
references:
  - >-
    packages/default/crates/cf-server/migrations/0330_derivation_environment_resolution.sql
  - packages/default/crates/cf-server/src/queries/build_jobs.rs
  - packages/default/crates/cf-server/src/queries/cache_push.rs
priority: medium
type: feature
ordinal: 569000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-508 fails closed when equally preferred registrations of one configuration are in different environments (migration 0330, derivation_environment). Build admission, claims, reuse and cache publication refuse the derivation with an explicit reason. This task designs and implements real shared-environment execution.

Required design points (user-stated):
1. One exact derivation may have several registered consumers across environments. Keep physical build deduplication. Never pick a registration by UUID.
2. Explicit build authorization for shared work. A NULL build_jobs.environment_id is claimable by every builder today, so it must not be used as a restricted scope. Shared jobs are claimable only by explicitly authorized builders.
3. Each environment keeps its own cache authorization. A shared artifact may be published to several authorized destinations. One environment's credentials are never exposed to another environment's builders. Do not pick one environment's cache arbitrarily.
4. Scans reuse physical evidence when identity and provenance are verified. Each registration still enforces its own environment access control. Configuration evidence never becomes Current runtime evidence.
5. Registration changes, environment moves and authorization revocation revalidate pending work.
6. If shared execution cannot be authorized safely, fail closed at admission with an actionable reason. Never return HTTP 500 at dispatch.
7. Tests: two registrations in different environments, build deduplication, authorized publication, successful scanning, restricted builders, restricted caches, registration removal and authorization changes.

Do not claim shared configurations work until the full build, publication and scan workflow passes integration tests.
<!-- SECTION:DESCRIPTION:END -->
