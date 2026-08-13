---
id: TASK-419
title: Optimize per-requirement compliance candidate discovery
status: Backlog
assignee: []
created_date: '2026-08-13 01:25'
labels:
  - phase-55
  - performance
  - compliance
dependencies: []
priority: medium
type: enhancement
ordinal: 414000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Phase 55 performance debt: find_policy_candidates currently scans accepted technical policies and trusted related mappings per imported requirement. Investigate batching or indexed candidate discovery for large STIG previews without changing precedence or trust semantics.
<!-- SECTION:DESCRIPTION:END -->
