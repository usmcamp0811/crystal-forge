---
id: TASK-410.1
title: Add real dashboard aggregate APIs for non-POA&M parity widgets
status: To Do
assignee: []
created_date: '2026-08-23 20:32'
labels:
  - dashboard
  - api
  - real-data
  - design-parity
  - non-poam
milestone: m-16
dependencies: []
references:
  - TASK-410
  - TASK-415
  - TASK-433.8
  - docs/design/CrystalForge/components/DashboardView.jsx
  - docs/design/CrystalForge/data-dashboard.js
documentation:
  - docs/agents/database-safety.md
  - docs/agents/verification.md
modified_files:
  - packages/default/crates/cf-server/src/api/models.rs
  - packages/default/crates/cf-server/src/handlers/api/dashboard.rs
  - packages/default/crates/cf-server/src/queries/dashboard.rs
  - packages/default/crates/cf-server/src/queries/flakes.rs
  - packages/web-ui/src/api/client.rs
  - packages/web-ui/src/api/models.rs
parent_task_id: TASK-410
priority: high
type: enhancement
ordinal: 442000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
TASK-410 must render the authoritative non-POA&M dashboard widgets from real product data, but current APIs do not provide complete or correctly scoped values for Build Queue worker/slot/failure metrics, successful evaluation counts, a unified chronological deployment activity feed, rich Flake Git Graph commit metadata/counts, or truthful cache health. Client-side fabrication or aggregation from limited/admin-only lists would violate the no-mock and authorization requirements.

## Goal
Provide the smallest authenticated, visibility-scoped server/API contracts required for TASK-410's non-POA&M dashboard widgets, preserving existing endpoint compatibility and using real persisted operational data.

## Scope
- Viewer-safe build aggregate with building/queued/failed-24h, active/total workers, and used/total slots.
- Unambiguous successful evaluation count separate from failures.
- Typed chronological deployment/pipeline activity entries backed by real deployment/build/evaluation events.
- Dashboard flake timeline enrichment with real commit message/author and real available build/evaluation status/count data.
- Truthful cache-health aggregate based only on existing persisted telemetry; optional capacity fields remain absent when no real source exists.
- Authorization and environment/system visibility scoping for every aggregate.

## Non-goals
- No POA&M summary/watchlist APIs or widgets; TASK-433 owns them.
- No deployment-approval or running-state-attestation APIs/widgets; TASK-415 owns them.
- No scanning, XCCDF, compliance, or policy-version changes; TASK-412 owns those areas.
- No fabricated counters, deterministic mock values, or new telemetry collection solely to imitate the design.
- No dashboard presentation work beyond client DTO/method support needed to consume these contracts.

## Architectural constraints
The server owns aggregate semantics, authorization, and visibility. Reuse existing queries/models where they are authoritative, avoid N+1 queries, preserve current clients, and add migrations only if genuinely required by a real persisted-data source.

## Risk
High: cross-cutting dashboard contracts and authorization-sensitive fleet aggregates.

## Verification
Targeted query/model/handler tests including visibility and empty states; server and web client formatting/checks; relevant Nix server/integration checks proportionate to changed interfaces.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 An authenticated visibility-scoped build aggregate returns real building queued failed-24h active/total worker and used/total slot values without requiring admin-only list access
- [ ] #2 Evaluation aggregate semantics expose successful completion separately from failures and preserve compatibility for existing consumers
- [ ] #3 A typed chronological dashboard activity API returns real deployment build and evaluation events with stable navigation metadata and deterministic ordering
- [ ] #4 Dashboard flake timeline entries include real commit message and author plus only real available build/evaluation status or count data
- [ ] #5 Cache health returns only states and metrics supported by real persisted telemetry and does not invent storage-capacity values
- [ ] #6 All new aggregates enforce current user environment/system visibility and authorization rules
- [ ] #7 Queries avoid per-system per-builder per-flake and per-cache N+1 behavior and have focused regression coverage
- [ ] #8 Existing dashboard and API consumers remain backward compatible
- [ ] #9 Targeted Rust tests and applicable Nix server/integration checks pass and exact commands are recorded
<!-- AC:END -->
