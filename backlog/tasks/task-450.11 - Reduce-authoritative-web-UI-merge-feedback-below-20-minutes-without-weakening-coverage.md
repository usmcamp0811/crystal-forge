---
id: TASK-450.11
title: >-
  Reduce authoritative web UI merge feedback below 20 minutes without weakening
  coverage
status: Backlog
assignee: []
created_date: '2026-09-01 03:12'
labels:
  - web-ui
  - testing
  - nix
  - playwright
  - ci-performance
dependencies:
  - TASK-438
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2807718751'
  - TASK-354
  - TASK-438
parent_task_id: TASK-450
priority: high
type: enhancement
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The authoritative `web-ui` merge-request check remains the longest blocking job. Pipeline 2807718751 was still running after 21 minutes even after the TASK-450 P0 build-graph changes. The check combines the embedded production server, the Dioxus WASM build, a large sequential Playwright workflow, screenshot capture, visual design-parity processing, and OSCAL and SARIF export validation.

Long feedback delays iteration and makes the web UI job the merge-request critical path.

## Goal

Return a trustworthy, merge-blocking web UI verdict in less than 20 minutes while preserving the existing production and coverage guarantees.

## Required guarantees

- A blocking pre-merge check must still exercise the production server binary serving the embedded production WASM through a real browser.
- Required browser-step failures must fail the merge gate and retain useful diagnostic artifacts.
- Runtime improvements must not remove semantic coverage, screenshot evidence, export validation, or design-parity evidence. Evidence that does not determine the merge verdict may complete in parallel.
- The result must remain reproducible in the repository Nix environment.

## Coordination

TASK-438 is the correctness prerequisite because runtime measurements are not meaningful while failed required browser steps can be reported as success. TASK-354 tracks existing Playwright flakiness and deterministic fixture issues that can otherwise prevent safe concurrency or tighter timing bounds.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Phase-level timing records distinguish Nix build or substitution time, VM startup and fixture setup, required Playwright execution, export validation, design-parity processing, and artifact publication
- [ ] #2 At least three representative merge-request pipeline runs produce the required blocking web UI verdict in less than 20 minutes of job execution time, with the median and maximum durations recorded
- [ ] #3 A blocking pre-merge check exercises the production server binary serving the embedded production WASM through a real browser
- [ ] #4 Every required selected browser-step failure causes the merge gate to fail and preserves the failed step name, reason, and available diagnostic artifacts
- [ ] #5 Existing required semantic browser coverage, screenshot evidence, OSCAL validation, SARIF validation, and design-parity evidence remain available after the runtime change
- [ ] #6 Runtime reduction does not depend on suppressing failures, unbounded retries, or shorter timeouts that make supported CI runners unreliable
- [ ] #7 The repository documents the authoritative web UI checks, their responsibilities, and the local Nix commands used to reproduce each check
<!-- AC:END -->
