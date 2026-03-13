---
id: TASK-39
title: Allow editing existing flakes
status: Done
assignee:
  - KimiK2.5
created_date: '2026-02-17 04:10'
updated_date: '2026-03-13 01:24'
labels:
  - ui
  - web-ui
  - flakes
milestone: m-10
dependencies: []
priority: high
ordinal: 37000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add UI flow in Flakes view to edit existing flake metadata (name and repository URL) after creation, including validation and save behavior.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented flake editing in Flakes view: added Edit action in table and card layouts, edit modal for name/repo updates, validation (required fields, git-like URL format, unique repo URL excluding current record), and state update/save flow.
<!-- SECTION:NOTES:END -->
