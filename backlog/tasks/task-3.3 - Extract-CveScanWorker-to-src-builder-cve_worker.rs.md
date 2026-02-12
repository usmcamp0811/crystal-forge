---
id: TASK-3.3
title: Extract CveScanWorker to src/builder/cve_worker.rs
status: To Do
assignee: []
created_date: '2026-02-04 21:12'
labels:
  - refactoring
  - builder
  - cve
  - rust
dependencies: []
parent_task_id: TASK-3
milestone: m-2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create new module for CVE scanning. Move run_cve_scan_loop and scan_derivations.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Create src/builder/cve_worker.rs
- [ ] #2 Define CveScanWorker struct
- [ ] #3 Implement scan_next_derivation() method
- [ ] #4 Implement run_scan_loop() background task
- [ ] #5 Add unit tests for scan orchestration
- [ ] #6 Update mod.rs to use CveScanWorker
<!-- AC:END -->
