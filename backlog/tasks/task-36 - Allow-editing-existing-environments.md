---
id: TASK-36
title: Allow editing existing environments
status: Done
assignee:
  - KimiK2.5
created_date: '2026-02-17 03:51'
updated_date: '2026-03-13 01:24'
labels:
  - ui
  - web-ui
  - environments
milestone: m-9
dependencies: []
priority: high
ordinal: 35000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add UI to edit environment metadata (name and description) after creation in Environments view, with validation and save flow.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented Edit Environment action with modal to update name/description after creation. Added UUID identity for environments to support safe renames and updates, and kept Edit Requirements + remove flows working with ID-based lookups.
<!-- SECTION:NOTES:END -->
