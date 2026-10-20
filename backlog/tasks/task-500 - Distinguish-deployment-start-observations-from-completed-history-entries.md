---
id: TASK-500
title: Distinguish deployment-start observations from completed history entries
status: To Do
assignee: []
created_date: '2026-10-20 05:20'
updated_date: '2026-10-20 20:30'
labels: []
dependencies: []
references:
  - packages/web-ui/src/views/system_detail.rs
  - docs/design/CrystalForge/components/SystemDetail.jsx
  - docs/knowledge/ui/deployment-history-rendering-rules.md
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
priority: medium
type: bug
ordinal: 551000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Correct the verified SystemDetail History and rollback-plan presentation defects against the approved reference and authoritative history contract. After two actual deployments the current view shows four deploys and renders Started observations as Deployed/success/no generation activated. The rollback plan renders Strategy rollback instead of approved switch_to_generation. The owner now authorizes only these bounded UI repairs, count semantics grounded in accepted authority, and behavioral Started/terminal/count/reload assertions. No icon, diagram, unrelated layout or design-reference changes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The accepted History presentation distinguishes Started from actual completed activation and defines the displayed deployment count.
- [ ] #2 The rollback plan's strategy label follows the approved presentation or an explicitly recorded product decision.
- [ ] #3 Behavioral browser coverage verifies Started versus completed presentation and durable reload behavior without synthetic completion evidence.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner authorization: current TASK-490 assignment selects TASK-500 for bounded product UI repair. Baseline hosted screenshot /tmp/opencode/hosted-delivery-history-df11c800.png and MR335 upload show the defect. Start a verified task-owned live preview before UI edits. Existing user's long-running :8080/:3445/:5432 instances must not be reused or stopped. Use dedicated worktree and native devenv isolation; coordinator owns preview processes/database and integration assertions.
<!-- SECTION:NOTES:END -->
