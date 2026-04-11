---
id: TASK-260
title: Improve "Link a flake" warning UX in Systems view
status: Done
assignee: []
created_date: '2026-04-11 00:00'
updated_date: '2026-04-11 03:12'
labels:
  - ux
  - systems-view
  - flakes
  - web-ui
milestone: m-12
dependencies: []
references:
  - packages/web-ui/src/views
  - packages/default/src/handlers/api/systems.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 860
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem Statement:
In Systems view, users see the warning: "1 system is not linked to a flake and won't be included in evaluations." The current "Link a flake" action navigates to the Flakes page without enough context about which system(s) are affected or what exact action clears the warning. This makes remediation non-obvious.

Goal:
Make the warning actionable in-place by clearly identifying impacted system(s) and guiding users to the exact remediation flow so they can link a flake and clear the warning.

Non-Goals:
- No backend data model changes for systems/flakes relationships.
- No redesign of the full Systems page layout.
- No new flake creation workflow.

Scope:
- Improve Systems-view warning content and action UX for unlinked systems.
- Show impacted system identifiers in the warning context.
- Provide a direct remediation path (or explicit guided steps) that makes the warning dismiss condition obvious.

Verification Plan:
- Update/extend web-ui integration checks to assert warning content includes impacted system context.
- Verify action path from warning leads to a resolvable linking flow.
- Run focused web-ui check(s) and relevant targeted tests in nix devshell.

Architectural Constraints:
- Keep business logic in backend/query/API layer; UI renders server-provided data and guidance.
- Follow existing web-ui component patterns and accessibility conventions.
- Keep scope tightly limited to unlinked-system warning UX.

Impact Areas:
- `packages/web-ui/src/views/systems*`
- `packages/web-ui/src/components/*` (if warning component reused)
- `checks/web-ui/tests/integration-test.js`
- Potentially related systems API warning payload mapping (no contract break unless explicitly updated)

Risk Level:
Medium (UX correctness and operator workflow; low data risk).

Dependencies:
- None.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Systems warning for unlinked flakes identifies affected system(s) (hostname and count at minimum).
- [ ] #2 Warning action provides an explicit remediation path to link a flake for the affected system(s).
- [ ] #3 After linking a flake to an affected system, warning no longer appears for that system on refresh.
- [ ] #4 UI copy clearly explains why the warning appears and what action resolves it.
- [ ] #5 Web-ui integration check(s) cover warning context and remediation-path behavior.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
MR !225 merged into dev: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/225

Pipeline for merge commit succeeded (warning state): https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2445502520
<!-- SECTION:NOTES:END -->
