---
id: TASK-36
title: Allow editing existing environments
status: Done
assignee: []
created_date: '2026-02-17 03:51'
updated_date: '2026-02-17 03:54'
labels:
  - ui
  - web-ui
  - environments
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add UI to edit environment metadata (name and description) after creation in Environments view, with validation and save flow.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented Edit Environment action with modal to update name/description after creation. Added UUID identity for environments to support safe renames and updates, and kept Edit Requirements + remove flows working with ID-based lookups.
<!-- SECTION:NOTES:END -->
