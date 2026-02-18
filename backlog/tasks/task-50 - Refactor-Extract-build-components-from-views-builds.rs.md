---
id: TASK-50
title: 'Refactor: Extract build components from views/builds.rs'
status: To Do
assignee: []
created_date: '2026-02-18 02:45'
labels:
  - refactoring
  - web-ui
  - builds
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The components/builds/ directory exists but is empty (just TODO comments). Components should be extracted from views/builds.rs.

## Components to Extract (from builds.rs)
Based on TODO in components/builds/mod.rs:
- MetricsRow
- MetricBadge
- WorkerStrip
- BuildQueuePane
- BuildDetailPane

## Acceptance Criteria
- [ ] Review views/builds.rs for extractable components
- [ ] Create component files in components/builds/
- [ ] Update components/builds/mod.rs with proper exports
- [ ] Update views/builds.rs to import from components
- [ ] Remove TODO comments from mod.rs
- [ ] Build passes: nix build .#checks.x86_64-linux.web-ui
<!-- SECTION:DESCRIPTION:END -->
