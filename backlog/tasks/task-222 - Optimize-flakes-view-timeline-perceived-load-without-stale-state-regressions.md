---
id: TASK-222
title: Optimize flakes view timeline perceived load without stale-state regressions
status: Backlog
assignee: []
created_date: '2026-03-29 00:47'
labels:
  - performance
  - ui
  - flakes
  - hotfix-followup
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

Flakes view can take ~10s before useful timeline content appears, creating a poor first-load experience. Prior optimization attempts caused severe regressions (browser lockups and empty timeline rendering despite API data).

## Goal

Improve perceived and actual timeline load speed on `/flakes` while preserving correctness and stability under production-like background activity.

## Context / Lessons Learned (must preserve)

The next implementation MUST explicitly avoid repeating prior failure modes:

1. **Do not rely on stale non-reactive memoization for timeline derivation**
   - A previous `use_memo` path captured an empty initial timeline state and prevented later async payloads from rendering.
   - Timeline derivation must always update when fresh timeline props/state arrive.

2. **Do not reintroduce mount-time churn that can lock browsers**
   - Avoid expensive all-flake/all-commit recomputation on every render.
   - Avoid aggressive auto-subscription behavior (e.g., websocket streams) at initial page mount when not strictly required.

3. **Preserve freshness guarantees for timeline fetches**
   - Keep request/cache semantics that prevent stale timeline responses from being reused across refreshes.

4. **Preserve correctness under async races**
   - Keep generation/epoch guards for progressive fetches so older responses cannot overwrite newer state.

## Proposed Approach (Sprint candidate)

Implement staged timeline loading focused on active flake UX:

- Stage A (fast first paint):
  - Fetch lightweight timeline summary for active flake (or selected/default flake) for latest N commits (e.g., 5).
  - Show commit list skeleton then immediate partial data.

- Stage B (active flake enrich):
  - Incrementally fetch additional active-flake commit details for visible/next commits only.
  - Continue background fetch while user remains on same flake.

- Stage C (cross-flake background):
  - Load non-active flakes lazily in bounded batches.
  - Cancel/ignore stale work when focus changes.

- Stage D (optional backend assist if needed):
  - If frontend-only changes cannot meet targets, add a dedicated summary endpoint/query shape for lightweight commit metadata (id/hash/message/author/timestamp/system_count).
  - Ensure DB query/index review is tied to measured bottlenecks.

## Non-Goals

- No broad UI redesign of flakes page.
- No unrelated refactors in other views.
- No schema changes unless query profiling proves required.

## Architectural Constraints

- Keep business logic out of presentation components.
- Keep async orchestration deterministic and race-safe (generation tokens / abort semantics).
- Keep network payloads scoped to what the current UI state needs.
- Keep rewrite-warning and existing sync flows functional.

## Verification Plan

- Reproduce baseline load timing on production-shaped dataset.
- Add/extend web-ui integration checks for staged timeline loading behavior and responsiveness.
- Validate no stale data overwrite when switching focused flake mid-load.
- Validate no browser freeze/regression under background updates.
- Run targeted web-ui checks and `nix build .#checks.x86_64-linux.web-ui`.

## Impact Areas

- `packages/web-ui/src/views/flakes_list.rs`
- `packages/web-ui/src/api/client.rs`
- `checks/web-ui/tests/integration-test.js`
- Optional backend query/handler paths only if profiling justifies.

## Risk Level

High (flakes view is user-critical and recently regressed badly).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 On `/flakes`, first meaningful timeline content for the active flake is visible within <=2s on production-shaped dataset in test environment.
- [ ] #2 Active flake initial load fetches only lightweight commit metadata for latest N commits (default 5) before deeper/background loading.
- [ ] #3 Switching focused flake cancels or safely ignores in-flight stale responses; UI never displays mismatched flake timeline data.
- [ ] #4 No browser lockups/crashes occur in Firefox or Brave during 2-minute interaction window on `/flakes`.
- [ ] #5 Timeline data updates correctly after sync/refresh and never gets stuck empty when API returns data.
- [ ] #6 `checks.x86_64-linux.web-ui` includes a regression scenario covering staged loading + focus-switch race and passes.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Documented anti-regression notes in task implementation notes and MR description (memoization pitfall, mount-time churn, race guards).
- [ ] #2 Measured before/after load timings attached to MR (active flake first-content timing).
- [ ] #3 No newly introduced flaky behavior in web-ui integration checks.
<!-- DOD:END -->
