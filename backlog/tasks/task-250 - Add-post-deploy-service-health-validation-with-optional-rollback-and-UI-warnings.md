---
id: TASK-250
title: >-
  Add post-deploy service health validation with optional rollback and UI
  warnings
status: To Do
assignee: []
created_date: '2026-04-08 17:40'
updated_date: '2026-06-10 03:23'
labels:
  - idea
  - deployment
  - systemd
  - ui
  - reliability
milestone: 'm-4: Advanced Features'
dependencies: []
modified_files:
  - packages/default/src/**
  - packages/web-ui/src/**
  - checks/web-ui/tests/integration-test.js
priority: low
ordinal: 14000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Crystal Forge currently treats a deployment as complete once the deploy action itself finishes, even if critical systemd services fail immediately afterward. This can leave systems in a technically deployed but operationally degraded state with little visibility in the UI.

## Goal
Design and implement post-deploy health validation for selected critical services, persist the resulting status, and surface actionable warnings and rollback guidance in the UI.

## Non-Goals
- No blind automatic rollback without explicit safety rules.
- No generic full observability platform or broad service-monitoring system.
- No expansion to arbitrary non-systemd health checks in this task.

## Scope
- Define how environments/systems specify critical services to validate after deploy.
- Run post-deploy validation for those services and store pass/fail/degraded results.
- Surface validation state and recommendations in UI deployment/status surfaces.
- Define manual-first rollback semantics, with optional automated rollback only if safety conditions are explicitly met.

## Architectural Constraints
- Health validation results must be auditable and tied to a specific deployment/generation.
- Rollback behavior must remain explicit, bounded, and safe by default.
- UI warnings must consume persisted backend state rather than inventing local heuristics.

## Impact Areas
- deployment pipeline/service validation path
- backend models/persistence for deployment health results
- system/deployment UI surfaces and warning banners
- optional rollback control flow and audit logging

## Verification Plan
Tier 1/2 depending on final implementation:
- targeted backend tests for validation result persistence and rollback decision logic
- targeted web-ui assertions/screenshots for warning states
- integration verification in repo dev environment with simulated healthy/unhealthy service outcomes
- if deployment pipeline/Nix integration changes materially, run the appropriate heavier Nix validation before closure

## Risk Level
Medium-High: touches deployment safety semantics and must avoid unsafe rollback automation.

## Dependencies
- Clear system/environment source for critical service lists.
- Existing deployment status surfaces available for warning integration.

## Replan note
This task was too lightweight for a To Do item. It has been expanded to Sprint-Ready quality so it can remain groomed without ambiguity.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A configurable list of critical systemd services can be checked automatically after deployment
- [ ] #2 Validation results (pass/fail/degraded) are persisted and available to the UI
- [ ] #3 UI displays post-deploy health state and clear warnings when checks fail
- [ ] #4 A rollback strategy is defined for failed validations, with manual-first behavior and explicit safety guardrails
- [ ] #5 Any automated rollback behavior is optional, explicitly bounded, and covered by tests
- [ ] #6 Verification demonstrates healthy and unhealthy post-deploy outcomes render correctly
<!-- AC:END -->
