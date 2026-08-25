---
id: TASK-410.1
title: Add real dashboard aggregate APIs for non-POA&M parity widgets
status: In Progress
assignee:
  - '@opencode-agent'
created_date: '2026-08-23 20:32'
updated_date: '2026-08-25 16:03'
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
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/320'
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
- [x] #1 An authenticated visibility-scoped build aggregate returns real building queued failed-24h active/total worker and used/total slot values without requiring admin-only list access
- [x] #2 Evaluation aggregate semantics expose successful completion separately from failures and preserve compatibility for existing consumers
- [x] #3 A typed chronological dashboard activity API returns real deployment build and evaluation events with stable navigation metadata and deterministic ordering
- [x] #4 Dashboard flake timeline entries include real commit message and author plus only real available build/evaluation status or count data
- [x] #5 Cache health returns only states and metrics supported by real persisted telemetry and does not invent storage-capacity values
- [x] #6 All new aggregates enforce current user environment/system visibility and authorization rules
- [x] #7 Queries avoid per-system per-builder per-flake and per-cache N+1 behavior and have focused regression coverage
- [x] #8 Existing dashboard and API consumers remain backward compatible
- [x] #9 Targeted Rust tests and applicable Nix server/integration checks pass and exact commands are recorded
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Restore the historical `active_builds` contract (non-terminal derivations) while applying viewer visibility only for authenticated callers.
2. Make worker availability the shared eligibility set for active workers, occupied slots, and total slot capacity; add exact visible/hidden and enabled/registered/active builder regressions.
3. Add authenticated dashboard handler regressions for viewer-scoped and no-membership empty states.
4. Add a repository-owned process-compose/Nix CI target that runs the ignored database regressions automatically on merge requests.
5. Run targeted non-web-ui verification, update committed task metadata, push review fixes, and confirm GitLab starts the required pipeline.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
AWAITING REVIEW: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/320

Implementation lock released after push and MR creation. Dedicated worktree retained pending review/merge.

Implemented and pushed commit `5cd2f762` on `TASK-410.1-dashboard-aggregates`; opened MR !320 targeting `dev`.

Delivered additive visibility-scoped build worker/slot/failure aggregates, explicit successful evaluation semantics, typed deterministic persisted activity, truthful cache health with capacity omitted, and a set-based enriched dashboard flake timeline. Non-admin queries fail closed through environment membership; focused isolated-DB coverage proves ambiguous configuration names do not leak hidden hostnames, global cache push activity is scoped, evaluation attempts remain stably ordered, hidden evaluation data is excluded, and the timeline avoids hidden failed-build influence.

Verification passed:
- server cargo formatting check
- offline cf-server cargo check
- WASM-target web-ui cargo check
- dashboard test filter (19 passed in default profile)
- API model tests (31 passed)
- focused ignored SQL regression against repository-owned isolated db-only PostgreSQL on 127.0.0.1:3042 (1 passed)
- `nix build .#packages.x86_64-linux.server --no-link`
- `git diff --check`

Per user instruction, no authoritative web-ui check was run locally; CI owns that check. No migration or SQLx offline metadata update was needed because all SQL is runtime query_as against existing schema.

MR !320 review requested changes. Resumed implementation in the existing TASK-410.1 worktree. Review-fix scope is limited to slot eligibility, active_builds compatibility, permanent authorization/empty-state regression coverage, CI wiring, and accurate task metadata. Per user instruction, the authoritative web-ui check will not be run locally.
<!-- SECTION:NOTES:END -->
