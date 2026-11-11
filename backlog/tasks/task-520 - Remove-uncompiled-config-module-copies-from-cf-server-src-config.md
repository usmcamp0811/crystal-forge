---
id: TASK-520
title: Remove uncompiled config module copies from cf-server/src/config
status: Backlog
assignee: []
created_date: '2026-11-11T21:10:00Z'
labels:
  - cleanup
  - server
dependencies: []
priority: low
type: chore
ordinal: 571000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found during TASK-519. `packages/default/crates/cf-server/src/config/mod.rs` re-exports `cf_config::config::*` and declares only `mod database_url_tests`. The sibling files (`agent.rs`, `auth.rs`, `builder.rs`, `build.rs`, `cache.rs`, `database.rs`, `deployment.rs`, `environment.rs`, `flakes.rs`, `oidc.rs`, `server.rs`, `system.rs`, `vulnix.rs`) are not compiled. They duplicate cf-config and can drift (TASK-519 had to edit `builder.rs` only to keep it from misleading readers). Confirm nothing includes them, then delete them.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Uncompiled duplicate config files under cf-server/src/config are removed
- [ ] #2 cf-server builds and its tests pass
<!-- AC:END -->
