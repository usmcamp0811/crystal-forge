---
id: TASK-310
title: Wire advanced deployment policies into deployment-manager enforcement path
status: Done
assignee: []
created_date: '2026-05-24 00:52'
updated_date: '2026-05-24 02:28'
labels:
  - deployment-manager
  - deployment-policies
  - enforcement
  - canary-rollout
  - approvals
  - time-window
  - cve-threshold
  - sprint-ready
milestone: STIG policy readiness
dependencies:
  - TASK-305
  - TASK-306
modified_files:
  - packages/default/src/deployment/mod.rs
  - packages/default/src/queries/derivations.rs
  - packages/default/src/services/cve_threshold_policy.rs
  - docs/deployment-policies.md
priority: high
ordinal: 257000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
Advanced deployment policies (`time_window`, `require_approvals`, `canary_rollout`, `cve_threshold`) are scaffolded (models, validation, services, and workflow APIs), but not yet fully enforced in the deployment-manager execution path. This creates a gap between policy configuration and runtime deployment gating.

## Goal
Integrate advanced policy evaluation into the deployment manager so deployment execution is deterministically gated by configured policies at runtime.

## Non-Goals
- No major UI implementation beyond API contract support already needed for execution
- No new policy types
- No broad compliance reporting/export bundle generation

## Scope
- Invoke advanced policy checks from deployment-manager pre-deploy gating path
- Enforce policy outcomes with consistent semantics:
  - `allow` => proceed
  - `warn` => proceed with warning recorded
  - `block` => stop deployment with explicit reason
  - `pending` (e.g., approvals/observation) => pause/deny until satisfied
- Ensure `require_approvals` gates deployment when insufficient valid approvals exist
- Ensure `canary_rollout` controls phase progression and selected system subsets in execution flow
- Ensure `time_window` and `cve_threshold` outcomes are respected during deployment decisions
- Record policy decision traces in deployment logs/events for operator visibility

## Architectural Constraints
- Keep deployment orchestration logic in deployment manager/domain layer (no business logic in UI)
- Reuse existing policy service modules; avoid duplicating evaluation logic
- Preserve clear boundary between API models, domain logic, and infrastructure/queries
- Maintain backward compatibility for deployments with no advanced policies configured

## Verification Plan (Tier 0)
- Targeted unit tests for deployment-manager gating decisions per policy outcome
- Targeted integration tests for approval-gated and canary-gated deployment scenarios
- Regression tests ensuring existing non-advanced policy behavior remains correct
- `nix develop -c cargo check`
- `nix develop -c cargo test <targeted modules>`

## Impact Areas
- Deployment manager orchestration modules
- Policy evaluation invocation path
- Deployment event/log recording for policy decisions
- Tests for deployment gating and rollout behavior

## Risk Level
High: direct impact on runtime deployment execution and gating correctness.

## Dependencies
- TASK-305 (advanced policy scaffolding)
- TASK-306 (canary small-fleet phase edge case) recommended before final merge of this task
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Deployment-manager pre-deploy path evaluates configured advanced policies for target deployment context
- [x] #2 Policy outcomes are enforced consistently (allow/warn/block/pending) with explicit operator-facing reason messages
- [x] #3 `require_approvals` blocks/pauses deployment until required valid approvals are present
- [x] #4 `canary_rollout` execution uses evaluated phase/system subset decisions from policy state
- [x] #5 `time_window` and `cve_threshold` outcomes are applied to runtime deployment gating decisions
- [x] #6 Targeted unit/integration tests cover each advanced policy affecting deployment-manager behavior
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Merged MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/263

Enforced advanced deployment policies in auto-latest manager with allow/warn/block/pending semantics.

Preserved legacy CVE gate compatibility and fixed fail-open enforcement gaps.
<!-- SECTION:FINAL_SUMMARY:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [x] #1 Document deployment-manager enforcement semantics for all advanced policy outcomes in deployment policy docs
<!-- DOD:END -->
