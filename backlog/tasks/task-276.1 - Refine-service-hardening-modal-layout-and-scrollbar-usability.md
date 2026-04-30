---
id: TASK-276.1
title: Refine service hardening modal layout and scrollbar usability
status: Backlog
assignee:
  - openai-gpt-5.4
created_date: '2026-04-23 13:23'
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

Desired outcome: Deliver a clean, design-standard modal pass that preserves current functionality while improving visual hierarchy, spacing, and scroll discoverability so users can reliably read full directive tables and add justifications without friction.
<!-- SECTION:DESCRIPTION:END -->
