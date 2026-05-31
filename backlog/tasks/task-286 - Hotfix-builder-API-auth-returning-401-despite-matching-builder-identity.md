---
id: TASK-286
title: Hotfix builder API auth returning 401 despite matching builder identity
status: Backlog
assignee: []
created_date: '2026-05-01 01:36'
labels:
  - bug
  - hotfix
  - auth
  - builder
  - production
milestone: Reliability
dependencies: []
references:
  - >-
    deployment log excerpt from reckless showing 401 on get-next-job and
    heartbeat
  - packages/default/src/handlers/
  - packages/default/src/services/
  - packages/default/src/builder/
priority: high
ordinal: 1500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Builder instances in the `reckless` deployment are repeatedly failing `get next job` and heartbeat calls with `401 Unauthorized`, even though the builder ID and derived public key shown in logs match the UI record. This blocks job execution and makes deployed builders non-functional.

## Goal
Identify and fix the authentication mismatch/regression so registered builders can poll and heartbeat successfully in production.

## Non-Goals
- Reworking builder registration UX
- Broad auth system redesign
- Unrelated queue/scheduler refactors

## Architectural Constraints
- Keep auth checks explicit and auditable
- Preserve existing API auth boundary (no bypasses)
- Minimize scope to builder auth/polling/heartbeat path

## Verification Plan
- Reproduce the 401 path with current code/config path used by `reckless`
- Validate successful poll + heartbeat auth after fix
- Run targeted server-side tests for builder auth path

## Risk
High (production hotfix affecting builder availability)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Builder `get next job` endpoint accepts valid builder auth for registered builder identity and no longer returns 401 for this scenario.
- [ ] #2 Builder heartbeat endpoint accepts the same valid identity in the same runtime session.
- [ ] #3 A targeted regression test covers the auth path that previously produced 401 in this deployment scenario.
- [ ] #4 No auth bypass is introduced; invalid credentials continue to return 401.
<!-- AC:END -->
