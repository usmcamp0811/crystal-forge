---
id: TASK-276.1
title: Refine service hardening modal layout and scrollbar usability
status: In Progress
assignee:
  - '@openai-gpt-5.4'
created_date: '2026-04-23 13:23'
updated_date: '2026-04-30 02:24'
labels:
  - ui
  - ux
  - hardening
  - follow-up
milestone: Security Visibility
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/242'
  - packages/web-ui/src/views/system_detail.rs
  - packages/web-ui/assets/app.css
documentation:
  - /home/mcamp/code/crystal-forge/design-example-systems
parent_task_id: TASK-276
priority: medium
ordinal: 4700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: The service hardening modal in the system detail view still feels visually rough and requires iterative polishing. Current pain points include header density/alignment, helper copy sizing consistency, and table scroll affordance clarity across browsers/viewports.

Goal: Deliver a clean, design-standard modal refinement pass that preserves current functionality while improving visual hierarchy, spacing, and scroll discoverability so users can reliably read full directive tables and add justifications without friction.

Non-Goals:
- No backend/API/schema changes.
- No changes to hardening scoring logic or data model.
- No redesign of unrelated System Detail tabs.

Architectural Constraints:
- Keep business logic out of UI views.
- Reuse existing web-ui components/patterns where possible.
- Scope changes to hardening modal presentation and interaction affordances only.

Verification Plan:
- Run targeted web-ui checks inside nix develop (format/lint/check/test as applicable for touched surface).
- Manually validate modal behavior and horizontal/vertical scroll affordance on typical desktop viewport.

Impact Areas:
- packages/web-ui/src/views/system_detail.rs
- packages/web-ui/assets/app.css

Risk Level: Low (UI-only, no API contract changes).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Service hardening modal header hierarchy is visually improved (service identity/posture prominence, spacing/alignment consistency) without removing existing information.
- [ ] #2 Directive table region provides clear scroll affordance (including horizontal overflow discoverability) and remains usable across standard desktop viewport widths.
- [ ] #3 Justification entry area is visually emphasized as the primary action area while preserving current submit behavior.
- [ ] #4 No regressions in opening/closing modal, directive rendering, or justification submission flow from System Detail hardening tab.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: openai-gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-276.1-hardening-modal-polish
<!-- SECTION:NOTES:END -->
