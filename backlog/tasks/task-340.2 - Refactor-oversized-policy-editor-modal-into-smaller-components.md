---
id: TASK-340.2
title: Refactor oversized policy editor modal into smaller components
status: Backlog
assignee: []
created_date: '2026-06-19 04:08'
labels:
  - technical-debt
  - policies
  - web-ui
  - architecture
milestone: 'm-20: Design Parity Missing Surfaces'
dependencies: []
references:
  - TASK-340.1
  - packages/web-ui/src/components/policy/policy_editor_modal.rs
modified_files:
  - packages/web-ui/src/components/policy/policy_editor_modal.rs
priority: medium
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
`packages/web-ui/src/components/policy/policy_editor_modal.rs` exceeds the repository architecture threshold for module size. During TASK-340.1 it measured about 1500 lines and already exceeded the 500-line module threshold before the parity work.

## Desired Outcome
Split the policy editor modal into smaller focused components/helpers while preserving existing API-backed behavior and test coverage.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Policy editor modal is split into focused modules/components under the policy component area
- [ ] #2 No user-visible policy editor behavior regresses
- [ ] #3 Existing policy web-ui checks continue to pass
<!-- AC:END -->
