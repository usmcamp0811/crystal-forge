---
id: TASK-357.1
title: Expose flake environment span data in registry API
status: Backlog
assignee: []
created_date: '2026-06-15 01:24'
labels:
  - flakes
  - backend
  - design-parity
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-357
priority: medium
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: The Flakes parity UI needs environment badges per flake, but the current flake registry API only returns id, name, repo_url, branch, build_scope, and system_count. Rendering environment badges from build_scope would fabricate data.

Desired Outcome: Extend the backend/API to expose authoritative environments spanned by each flake so the Flakes list/table/card surfaces can render real environment badges.
<!-- SECTION:DESCRIPTION:END -->
