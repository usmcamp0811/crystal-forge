---
id: TASK-52
title: 'Refactor: Extract policy components from views/policies.rs'
status: To Do
assignee: []
created_date: '2026-02-18 02:46'
updated_date: '2026-02-19 03:53'
labels:
  - refactoring
  - web-ui
  - policy
dependencies: []
priority: low
milestone: m-13
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
<!-- AC:BEGIN -->
- [ ] #1 Review views/policies.rs for extractable components
- [ ] #2 Create component files in components/policy/
- [ ] #3 Update components/policy/mod.rs with proper exports
- [ ] #4 Update views/policies.rs to import from components
- [ ] #5 Remove TODO comments from mod.rs
- [ ] #6 Build passes: nix build .#checks.x86_64-linux.web-ui
<!-- SECTION:DESCRIPTION:END -->
<!-- AC:END -->
