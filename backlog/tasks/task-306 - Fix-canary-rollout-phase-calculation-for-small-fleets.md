---
id: TASK-306
title: Fix canary rollout phase calculation for small fleets
status: Backlog
assignee: []
created_date: '2026-05-23 23:43'
labels:
  - bug
  - canary-rollout
  - edge-case
dependencies: []
priority: low
ordinal: 253000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Canary rollout phase count is calculated as `ceil(100 / percentage)`, which doesn't account for actual fleet size. This causes issues with small fleets:

**Example:**
- Fleet: 3 systems
- Percentage: 25%
- Calculated phases: ceil(100/25) = 4
- Actual batches needed: 3 (since 25% of 3 = 0.75, rounds to 1 per phase)
- Result: Final phase is empty

This produces a confusing "phase 4/4 with 0 systems" state.

## Current Code Location

`packages/default/src/services/canary_rollout.rs` - `init_rollout()` function calculates:
```rust
let total_phases = (100.0 / config.percentage as f64).ceil() as i32;
```

This is used before any system selection happens, so it doesn't know the actual fleet size.

## Desired Behavior

Either:
1. Calculate phases from actual batch count: `total_phases = number of non-empty batches selected`
2. Complete rollout early when `remaining_systems.is_empty()` instead of advancing to empty phase
3. Use `min(ceil(100 / percentage), ceil(fleet_size / batch_size))` during init

Option 2 is probably simplest: in `advance_to_next_phase()`, check if no systems remain and mark as completed instead of creating an empty phase.

## Impact

- **Low priority**: Only affects small fleets (< ~10 systems)
- **No security risk**: Empty phase just looks confusing
- **Workaround exists**: Use larger percentage values for small fleets

## Context

This issue was identified during MR #262 review. Deployment-manager enforcement is follow-up work, so this doesn't block the scaffolding MR.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Canary rollout with 3 systems at 25% does not create empty final phase
- [ ] #2 Rollout completes when no remaining systems exist, regardless of calculated phase count
- [ ] #3 Phase count accurately reflects actual deployment batches, not just percentage math
<!-- AC:END -->
