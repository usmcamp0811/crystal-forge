---
id: TASK-52
title: 'Refactor: Extract policy components from views/policies.rs'
status: To Do
assignee: []
created_date: '2026-02-18 02:46'
labels:
  - refactoring
  - web-ui
  - policy
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The components/policy/ directory exists but is empty (just TODO comments). Policy components should be extracted from views/policies.rs.

## Components to Extract
Based on TODO in components/policy/mod.rs:
- PolicyCard
- PolicyEditorModal

## Acceptance Criteria
- [ ] Review views/policies.rs for extractable components
- [ ] Create component files in components/policy/
- [ ] Update components/policy/mod.rs with proper exports
- [ ] Update views/policies.rs to import from components
- [ ] Remove TODO comments from mod.rs
- [ ] Build passes: nix build .#checks.x86_64-linux.web-ui
<!-- SECTION:DESCRIPTION:END -->
