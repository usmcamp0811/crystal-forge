---
id: TASK-177
title: Preserve policy editor state when toggling Basic/Advanced views
status: Backlog
assignee: []
created_date: '2026-03-09 01:23'
labels:
  - web-ui
  - policy-editor
  - ux
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: In the policy editor, switching between Basic and Advanced views resets or mutates in-progress field values, causing users to lose edits.

Desired outcome: Toggling between Basic and Advanced should be a lossless translation of the same underlying policy/check model. User-entered values must be preserved across view switches, and only representation should change.
<!-- SECTION:DESCRIPTION:END -->
