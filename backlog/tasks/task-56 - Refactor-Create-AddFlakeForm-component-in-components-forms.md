---
id: TASK-56
title: 'Refactor: Create AddFlakeForm component in components/forms/'
status: To Do
assignee: []
created_date: '2026-02-18 02:47'
labels:
  - refactoring
  - web-ui
  - forms
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The components/forms/ directory has TODO comments indicating AddFlakeForm should be extracted from views.

## Components to Create
Based on TODO in components/forms/mod.rs:
- AddFlakeForm (extract from flakes_list.rs)

## Acceptance Criteria
- [ ] Identify AddFlakeForm or similar form in views/flakes_list.rs
- [ ] Create components/forms/add_flake_form.rs
- [ ] Update components/forms/mod.rs with export
- [ ] Update views to import from components
- [ ] Remove TODO comments from mod.rs
- [ ] Build passes: nix build .#checks.x86_64-linux.web-ui
<!-- SECTION:DESCRIPTION:END -->
