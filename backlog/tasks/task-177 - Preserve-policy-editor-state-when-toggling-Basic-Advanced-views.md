---
id: TASK-177
title: Preserve policy editor state when toggling Basic/Advanced views
status: Done
assignee: []
created_date: '2026-03-09 01:23'
updated_date: '2026-03-13 01:24'
labels:
  - web-ui
  - policy-editor
  - ux
dependencies: []
priority: high
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: In the policy editor, switching between Basic and Advanced views resets or mutates in-progress field values, causing users to lose edits.

Desired outcome: Toggling between Basic and Advanced should be a lossless translation of the same underlying policy/check model. User-entered values must be preserved across view switches, and only representation should change.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation Found

This task was already implemented in commit e27430b0 (merged to dev on 2026-03-08):

```
fix: preserve policy data when switching basic and advanced authoring modes

Translate current basic builder state into advanced JSON payload on mode switch, and parse advanced payload back into basic builder fields when returning to basic mode so user input is retained instead of reset.
```

Implementation:
- Translates basic builder state → advanced JSON on mode switch
- Parses advanced JSON → basic builder fields when returning to basic mode
- User input retained across view toggles (lossless translation)

File changed: packages/web-ui/src/components/policy/policy_editor_modal.rs (+181 lines)

Task marked Done (already merged).
<!-- SECTION:NOTES:END -->
