---
id: TASK-1.1
title: Add DeploymentStrategy enum to config/deployment.rs
status: To Do
assignee: []
created_date: '2026-02-04 20:19'
labels:
  - deployment
  - config
  - rust
dependencies: []
parent_task_id: TASK-1
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create enum with ImmediatePersist and BootOnly variants. Add Default trait implementation defaulting to ImmediatePersist.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Define DeploymentStrategy enum with serde annotations
- [ ] #2 Implement Default trait
- [ ] #3 Add to DeploymentConfig struct with #[serde(default)]
- [ ] #4 Add unit tests for enum serialization/deserialization
<!-- AC:END -->
