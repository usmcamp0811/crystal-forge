---
id: TASK-115
title: 'Refactor admin handler: extract queries module and split API admin.rs'
status: Backlog
assignee: []
created_date: '2026-02-22 17:14'
labels: []
dependencies: []
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: packages/default/src/handlers/api/admin.rs has grown to ~1500 lines and contains many inline sqlx queries, which conflicts with repository architecture guidance (module size limits and query-layer separation). Desired Outcome: move SQL into packages/default/src/queries/admin.rs (and submodules if needed), split admin handler by concern (users, mappings, audit, memberships), and keep handler layer focused on orchestration with test coverage preserved.
<!-- SECTION:DESCRIPTION:END -->
