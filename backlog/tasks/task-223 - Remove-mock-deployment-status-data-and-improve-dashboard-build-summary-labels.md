---
id: TASK-223
title: Remove mock deployment-status data and improve dashboard build summary labels
status: Backlog
assignee: []
created_date: '2026-03-29 03:01'
updated_date: '2026-03-29 03:04'
labels:
  - dashboard
  - ui
  - bug
  - data-correctness
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

Dashboard still contains mock-style behavior in widgets adjacent to Fleet Health:

1. **Deployment Status** shows fabricated system names (e.g., `new-server-01`) and synthetic per-flake count transformations.
2. **Build Summary** hover/system labels are not operator-friendly when identifiers are long; current display should be normalized to a concise, meaningful format.

This causes misleading operator data and inconsistent dashboard trustworthiness.

## Goal

Make Dashboard Deployment Status and Build Summary fully data-correct and operator-usable:
- no fabricated system names or synthetic transformed counts,
- concise build labels that prioritize useful identity (e.g., `<flake-name> <nixosConfig/hostname>`), not long raw identifiers.

## Non-Goals

- No broad dashboard redesign.
- No backend schema changes.
- No changes to unrelated widgets.

## Architectural Constraints

- Keep dashboard widgets presentational; data shaping should remain deterministic and minimal.
- Do not fabricate entities in UI when backend only provides aggregate counts.
- Preserve existing API contracts unless absolutely required.

## Verification Plan

- Confirm Deployment Status uses API-provided counts directly and no hardcoded hostnames appear.
- Confirm Build Summary labels are concise and deterministic for queued/building entries.
- Run targeted web-ui checks and `nix build .#checks.x86_64-linux.web-ui`.

## Impact Areas

- `packages/web-ui/src/components/dashboard/deployment_status.rs`
- `packages/web-ui/src/components/dashboard/build_summary.rs`
- Optional dashboard tests/integration snapshots.

## Risk Level

Medium (dashboard correctness + usability).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Deployment Status widget no longer displays fabricated system names (including `new-server-01`) and no longer applies synthetic per-flake count math.
- [ ] #2 Deployment Status counts shown in donut/legend match backend `DeploymentStatusSummary` values for the session.
- [ ] #3 Build Summary hover/system labels use a concise operator-friendly format (prefer flake + config/hostname) and avoid long unhelpful raw identifiers.
- [ ] #4 No fallback fake entity names are shown in either widget when data is absent; empty states are explicit and non-misleading.
- [ ] #5 `nix build .#checks.x86_64-linux.web-ui` passes with dashboard widget behavior validated.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Selected for immediate execution per maintainer instruction: include this in current dashboard correctness push.

LOCK: opencode-gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-223-dashboard-widget-data-correctness

Execution paused before code changes: maintainer requested this work be folded into TASK-201 MR193 instead.
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 MR includes before/after screenshot of dashboard widgets.
- [ ] #2 Any additional API-data gaps discovered are captured as separate Backlog tasks.
<!-- DOD:END -->
