---
id: TASK-358.1
title: Split oversized environments query module
status: Backlog
assignee: []
created_date: '2026-06-14 19:37'
labels:
  - backend
  - architecture
  - technical-debt
  - environments
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - packages/default/src/queries/environments.rs
priority: medium
ordinal: 306000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
`packages/default/src/queries/environments.rs` already exceeded the 500-line architecture threshold before TASK-358 and grew further while adding environment rollup query support. The module mixes environment CRUD/list queries, policy queries, system effective policy helpers, and tests.

## Desired Outcome
Split `queries/environments.rs` into focused submodules (for example: `summary`, `policies`, `system_policy`, and `tests`) while preserving public API compatibility. Keep behavior unchanged and add/retain targeted tests during the split.
<!-- SECTION:DESCRIPTION:END -->
