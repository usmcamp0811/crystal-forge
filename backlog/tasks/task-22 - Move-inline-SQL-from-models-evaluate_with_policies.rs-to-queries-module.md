---
id: TASK-22
title: Move inline SQL from models/evaluate_with_policies.rs to queries/ module
status: Backlog
assignee: []
created_date: '2026-02-14 00:24'
updated_date: '2026-02-19 03:39'
labels:
  - refactoring
  - sql
  - architecture
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found 1 inline sqlx::query! in models/evaluate_with_policies.rs (line ~293) that updates derivation status to DryRunComplete. Should be extracted to queries/derivations.rs to maintain the project convention that all SQL lives in the queries/ module. This is part of a larger evaluate function so may need careful refactoring.
<!-- SECTION:DESCRIPTION:END -->
