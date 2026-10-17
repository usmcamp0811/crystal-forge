---
id: TASK-490.24
title: >-
  Coordinator: four-owner cutover, CI entry point, and legacy integration
  retirement
status: Backlog
assignee: []
created_date: '2026-10-17 16:39'
labels:
  - testing
  - integration
  - coordinator
dependencies:
  - TASK-490.2
  - TASK-490.3
  - TASK-490.4
  - TASK-490.5
  - TASK-490.6
  - TASK-490.8
  - TASK-490.9
  - TASK-490.10
  - TASK-490.11
  - TASK-490.12
  - TASK-490.13
  - TASK-490.14
  - TASK-490.15
  - TASK-490.16
  - TASK-490.17
  - TASK-490.18
  - TASK-490.19
  - TASK-490.20
  - TASK-490.21
  - TASK-490.22
  - TASK-490.23
parent_task_id: TASK-490
priority: high
type: task
ordinal: 536000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Coordinator-owned (not a Luna packet). Writes checks/integration-{access,delivery,assurance,upgrade}/default.nix, tests/integration/<owner>/default.nix (required, complete), .gitlab-ci.yml integration entry point, legacy check removal, coverage map, contract status, and related docs.

WHY: the final inventory is exactly four integration checks; legacy registrations and CI selections stay until replacements pass.

SCOPE: for each owner whose required set is implemented, set complete = true and add checks/integration-<owner>/default.nix returning the owner's `all`; verify `nix eval .#checks.x86_64-linux --apply builtins.attrNames` shows exactly the four integration names plus the classified non-integration checks; add the four-name allowlist comparison at the start of the existing integration CI entry point; run each owner's complete set and record normal runtime and peak resources; retire each legacy registration and CI selection only after its replacement passes (or an owner-approved scope change exists); move checks/web-ui/tests/fixtures/*.xml to tests/integration/assurance/fixtures/; update docs/knowledge/testing/flake-checks.md, web-ui-check.md, the coverage map, and the contract implementation_status. Final cutover stays blocked while any required replacement fails, including because of a product defect, unless the owner changes scope.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Exactly four integration checks are exported and each runs its complete required scenario set, propagating every failure
- [ ] #2 CI runs the four owners and checks the four-name allowlist without a new validation service
- [ ] #3 Each retired legacy check has a passing replacement recorded in the coverage map or an owner-approved scope change
- [ ] #4 Non-integration checks keep their own attributes and classification
- [ ] #5 Runtime and peak-resource figures are recorded per owner
<!-- AC:END -->
