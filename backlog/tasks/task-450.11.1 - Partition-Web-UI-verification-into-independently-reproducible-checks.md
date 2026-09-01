---
id: TASK-450.11.1
title: Partition Web UI verification into independently reproducible checks
status: Backlog
assignee: []
created_date: '2026-09-01 03:27'
labels:
  - web-ui
  - testing
  - nix
  - playwright
  - ci-performance
dependencies:
  - TASK-438
references:
  - TASK-354
  - TASK-430
  - checks/web-ui/default.nix
  - checks/web-ui/coverage-manifest.json
  - checks/web-ui/tests/integration-test.js
parent_task_id: TASK-450.11
priority: high
type: enhancement
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The current Web UI Nix check combines the production embedding smoke guarantee, required semantic browser workflows, OSCAL and SARIF export validation, and advisory design-parity processing in one serial VM test. This prevents GitLab from distributing independent work across available runners and makes one failure rerun the complete check.

## Goal

Expose a small set of independently reproducible Nix checks with explicit responsibilities. Together, the blocking checks must preserve the existing authoritative Web UI merge gate. Advisory visual evidence must remain available without extending the blocking critical path.

## Required topology

- One blocking production smoke check exercises the production server binary, embedded production WASM, and a real browser.
- Required semantic browser workflows are divided into a small number of balanced, blocking groups with deterministic isolated state.
- Browser-based OSCAL and SARIF export validation has an independently runnable responsibility.
- Design-parity processing has an independently runnable advisory responsibility.

The task must preserve shared derivation identity so separate checks can reuse Nix-built inputs. Do not reduce coverage or hide failed required steps.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A blocking Nix check exercises the production server binary serving the embedded production WASM through a real browser
- [ ] #2 All required semantic browser workflows belong to exactly one documented blocking check group and no required workflow is omitted
- [ ] #3 Browser workflow groups use deterministic isolated state and can run in any order or concurrently without cross-group state dependencies
- [ ] #4 OSCAL and SARIF browser export validation can run independently from the required semantic browser workflow groups and remains blocking
- [ ] #5 Design-parity evidence can run independently from the blocking semantic and export checks and retains its documented advisory behavior
- [ ] #6 Each check has a stable flake attribute and an exact documented Nix command that reproduces it locally
- [ ] #7 Shared server, Web UI, browser, fixture, and test-environment inputs retain reusable derivation paths across the checks rather than being rebuilt from copied definitions
- [ ] #8 A deliberately failing required browser step fails only its responsible check and preserves its step name, reason, and available artifacts
- [ ] #9 The existing `web-ui` flake attribute has an explicitly documented compatibility role and does not silently weaken the complete merge gate
<!-- AC:END -->
