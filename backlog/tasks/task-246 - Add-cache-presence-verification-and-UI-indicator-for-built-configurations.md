---
id: TASK-246
title: Add cache presence verification and UI indicator for built configurations
status: Backlog
assignee: []
created_date: '2026-04-05 22:14'
labels:
  - cache
  - monitoring
  - frontend
  - backend
  - observability
  - ux
dependencies: []
references:
  - packages/web-ui/src/views/flake_detail.rs
  - packages/default/src/queries/builds.rs
  - packages/default/src/handlers/api/caches.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Users and operators have no visibility into whether a previously built configuration is still available in the configured cache. This creates uncertainty during deployment planning and troubleshooting:

- A build may have completed successfully weeks ago, but the cache entry could have been evicted, deleted, or expired
- Users viewing the Flake detail view cannot tell if a configuration is immediately deployable or needs to be rebuilt
- No automated verification runs to detect cache drift or validate cache retention policies
- Operators cannot distinguish between "never built" vs "built but cache entry lost" vs "built and cached"

This information is critical for:
- Deployment readiness assessment
- Cache health monitoring
- Troubleshooting deployment failures
- Understanding infrastructure state

## Goal

Implement periodic cache presence verification for built configurations and surface this information in the Flake detail UI, so users and operators can see at-a-glance whether a configuration is cached and immediately deployable.

## Non-Goals

- This task does NOT implement automatic cache repopulation or rebuilding
- This task does NOT change cache eviction policies
- This task does NOT add cache warming or preemptive builds
- This task does NOT modify the build queue or builder behavior
- This task does NOT implement cache health scoring or analytics beyond presence/absence

## Scope

1. Add a periodic background job that checks cache presence for recent/active built configurations
2. Store cache presence status and last-verified timestamp in the database
3. Add UI indicator in Flake detail view showing cache status per system configuration
4. Provide clear visual distinction between: cached (green), not cached (yellow/warning), never built (gray), and unknown/stale verification (gray with timestamp)

## Architectural Constraints

- Cache presence checks MUST NOT block user requests or UI rendering
- Verification job MUST be rate-limited to avoid overwhelming cache infrastructure
- Database schema MUST support timestamp-based staleness detection
- UI indicator MUST gracefully handle missing/stale verification data
- Backend verification logic MUST be decoupled from specific cache backend types where possible
- Avoid N+1 query patterns when loading cache status for multiple configurations
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Background job periodically verifies cache presence for built configurations and updates database with results and timestamp
- [ ] #2 Flake detail view displays cache presence indicator for each system configuration with clear visual states (cached/not-cached/never-built/unknown)
- [ ] #3 Cache presence status includes last-verified timestamp so users can assess staleness
- [ ] #4 Verification job respects rate limits and does not impact cache performance
- [ ] #5 UI gracefully handles missing or stale cache verification data
- [ ] #6 Database schema supports storing cache presence status and verification timestamp per built configuration
<!-- AC:END -->
