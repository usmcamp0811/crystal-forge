---
id: TASK-511
title: Remove or gate the unused legacy build_reservations claim path
status: Backlog
assignee: []
created_date: '2026-10-31T21:36:00Z'
labels: []
dependencies: []
priority: low
type: chore
ordinal: 562000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found during TASK-508 review. The exported `run_build_loop` calls `build_reservations::claim_next_derivation`, which reads `view_buildable_derivations` without the deployment-agent or artifact-admission policy gate that every API builder peek/claim selector enforces. No binary startup path calls it today, so it is dead but unsafe if re-enabled. Deleting it (preferred) or gating it removes a latent bypass of build admission policy.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The legacy loop and claim function are removed, or they enforce the same admission predicate as queries/builders.rs claim selectors
- [ ] #2 No production build path can claim a derivation without admission evidence
- [ ] #3 cf-server tests pass
<!-- AC:END -->
