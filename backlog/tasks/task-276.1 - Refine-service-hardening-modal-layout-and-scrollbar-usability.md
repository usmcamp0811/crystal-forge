---
id: TASK-276.1
title: Refine service hardening modal layout and scrollbar usability
status: To Do
assignee:
  - agent
created_date: '2026-04-23 13:23'
updated_date: '2026-06-10 02:53'
labels:
  - frontend
  - ux
  - hardening
milestone: Sprint 37
dependencies: []
references:
  - >-
    /home/mcamp/code/crystal-forge/crystal-forge/project/components/HardeningTab.jsx
  - /home/mcamp/code/crystal-forge/crystal-forge/project/styles.css
  - >-
    /home/mcamp/code/crystal-forge/TASK-276.1-hardening-modal-polish/packages/web-ui/src/views/system_detail.rs
  - >-
    /home/mcamp/code/crystal-forge/TASK-276.1-hardening-modal-polish/packages/web-ui/assets/app.css
  - >-
    /home/mcamp/code/crystal-forge/TASK-276.1-hardening-modal-polish/checks/web-ui/tests/integration-test.js
documentation:
  - /home/mcamp/code/crystal-forge/design-example-systems
parent_task_id: TASK-276
priority: high
ordinal: 16000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Polish the System Detail Hardening tab and service hardening modal UX to align with the approved design direction. Focus on readability, hierarchy, and discoverability for long directive lists while preserving existing hardening data semantics.

Problem
- Current hardening tab and modal behavior is functionally correct but visual hierarchy and flow are harder to scan than the target design reference.
- Users need clearer cues for risk distribution, filtering, and detail drill-down.
- Long directive lists in the modal need stronger scroll affordance and helper context.

Goal
- Match the hardening tab and modal interaction model to the design reference style.
- Improve service summary readability, filtering affordance, and action labeling.
- Ensure modal details (directives + justification) are easy to navigate and understand.

Non-Goals
- No backend API/schema changes.
- No hardening scoring algorithm changes.
- No cross-page redesign outside hardening tab/modal and related screenshot test steps.

Architectural Constraints
- Keep business logic out of view components.
- Reuse existing DTOs/hooks/state flow.
- Keep changes scoped to web-ui hardening presentation and associated checks.

Impact Areas
- packages/web-ui/src/views/system_detail.rs
- packages/web-ui/assets/app.css
- checks/web-ui/tests/integration-test.js

Verification Plan
- Tier 0/1 focused verification for web-ui:
  - nix develop -c cargo check (web-ui workspace)
  - nix develop -c cargo test (web-ui workspace)
  - nix build .#checks.x86_64-linux.web-ui
- Confirm hardening screenshots captured from web-ui check for:
  - hardening fleet route
  - system detail hardening tab/modal
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Hardening tab visual hierarchy aligns with approved design direction (summary stats, risk filters, service table action labels).
- [x] #2 Service hardening modal remains readable and usable for long directive lists (scroll affordance + clear helper guidance).
- [x] #3 checks/web-ui captures hardening-related screenshots successfully (especially fleet hardening + system hardening tab/modal flow).
- [x] #4 Existing web-ui build/test checks remain passing after scoped updates.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Opened MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/246

Rebased branch on dev before final verification and MR creation.

Verification executed: nix develop -c cargo check (web-ui), nix develop -c cargo test (web-ui), nix build .#checks.x86_64-linux.web-ui. Hardening screenshots now include 27-hardening-fleet and 28-system-hardening-tab.
<!-- SECTION:NOTES:END -->
