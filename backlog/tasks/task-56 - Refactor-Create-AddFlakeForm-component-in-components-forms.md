---
id: TASK-56
title: 'Refactor: Create AddFlakeForm component in components/forms/'
status: Backlog
assignee: ["KimiK2.5"]
created_date: '2026-02-18 02:47'
updated_date: '2026-02-19 03:39'
labels:
  - refactoring
  - web-ui
  - forms
dependencies: []
priority: low
milestone: m-10
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The components/forms/ directory has TODO comments indicating AddFlakeForm should be extracted from views.

## Components to Create
Based on TODO in components/forms/mod.rs:
- AddFlakeForm (extract from flakes_list.rs)

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Identify AddFlakeForm or similar form in views/flakes_list.rs
- [ ] #2 Create components/forms/add_flake_form.rs
- [ ] #3 Update components/forms/mod.rs with export
- [ ] #4 Update views to import from components
- [ ] #5 Remove TODO comments from mod.rs
- [ ] #6 Build passes: nix build .#checks.x86_64-linux.web-ui
<!-- SECTION:DESCRIPTION:END -->
<!-- AC:END -->
