---
id: TASK-3.6
title: Create BuilderError types in src/builder/error.rs
status: Backlog
assignee:
  - '@Matt'
created_date: '2026-02-04 21:12'
updated_date: '2026-02-20 18:12'
labels:
  - refactoring
  - builder
  - error-handling
  - rust
milestone: m-2
dependencies: []
parent_task_id: TASK-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Define domain-specific error types for builder module.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Create src/builder/error.rs
- [ ] #2 Define BuilderError enum with variants
- [ ] #3 Implement Display and Error traits
- [ ] #4 Add error context helpers
- [ ] #5 Replace anyhow::Error in builder modules
- [ ] #6 Add unit tests for error conversion
<!-- AC:END -->
