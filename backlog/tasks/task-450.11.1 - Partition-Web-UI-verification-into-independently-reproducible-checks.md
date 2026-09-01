---
id: TASK-450.11.1
title: Partition Web UI verification into independently reproducible checks
status: In Progress
assignee:
  - opencode-gpt-5.6-sol
created_date: '2026-09-01 03:27'
updated_date: '2026-09-01 18:07'
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
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/325'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add an explicit Web UI check-group ownership manifest and a fast Node validator. Partition every `ci_fast` step exactly once into ordered fleet, pipeline, and governance groups; keep state chains together; define separate compatibility and design-parity selections; reject unknown profiles, unknown requested steps, duplicate ownership, omissions, and requested steps excluded by the selected profile.
2. Refactor `checks/web-ui/default.nix` into the shared parameterized VM/evidence constructor. Keep the `web-ui` wrapper as the production `cf-server-drv` embedded-WASM asset and real-Chromium shell compatibility check. Add stable Snowfall wrappers for fleet, pipeline, governance, blocking browser exports, and advisory design parity while preserving identical shared package derivations.
3. Make each VM derivation produce evidence independently of its logical gate verdict. Preserve infrastructure failures as evidence failures, copy `results.json`, verdicts, failed-step screenshots, export artifacts, and design reports, and expose the evidence derivation through each blocking check's `.evidence` passthru so CI can build/copy it without rerunning the VM.
4. Update named-profile authentication preflight and selection validation in `integration-test.js`. Preserve ordered state chains and keep full-only onboarding outside ordinary blocking groups.
5. Document stable attributes, compatibility guarantees, ownership invariants, evidence/gate behavior, and exact local commands. Run Node tests and syntax checks, the static ownership validator, Nix parsing, check-name evaluation, and cheap derivation evaluations without building the VM checks.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
The user selected this task for one focused Web UI optimization MR with TASK-438, TASK-354, TASK-450.11.2, and TASK-450.11.3.

LOCK: opencode-gpt-5.6-sol in /home/mcamp/code/crystal-forge/TASK-450-web-ui-parallel-checks on branch TASK-450.11-web-ui-parallel-checks, based on TASK-450-p0-build-graph at 437efd55.

Implemented the Nix/check partition on top of the existing uncommitted harness reliability changes. Added explicit `ci_fast` ownership (fleet 32, pipeline 33, governance 35), named-profile selection/auth validation, stable Snowfall wrappers for production compatibility, required groups, exports, and advisory design parity, and an evidence/gate split with `.evidence` passthru. Production `web-ui` continues to use `cf-server-drv`, `verifyWebUiAssets`, the packaged Web UI, and real Chromium. Verified Node syntax/tests and static ownership in `nix develop`, parsed all six Nix definitions, evaluated all stable check and evidence derivations, compiled generated NixOS Python test scripts for normal/export/design variants, and confirmed shared input derivation paths are identical. No VM builds, commit, push, or `.gitlab-ci.yml` change were performed.
<!-- SECTION:NOTES:END -->
