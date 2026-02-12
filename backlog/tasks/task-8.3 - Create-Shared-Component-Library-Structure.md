---
id: TASK-8.3
title: Create UI Module Structure within packages/default
status: To Do
assignee: []
created_date: '2026-02-05 14:15'
labels:
  - ui
  - architecture
dependencies:
  - TASK-8.1
parent_task_id: TASK-8
priority: high
milestone: m-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Set up the module structure within packages/web-ui/ for the Dioxus web application. TUI has been deferred; this is web-only.

Note: The web UI lives in a separate crate (packages/web-ui/) because the server crate has native-only deps (sqlx, sysinfo, nix) incompatible with wasm32. API DTOs live in packages/default/src/api/models.rs and are duplicated as equivalent types in the web-ui crate for JSON wire compatibility.

Steps:
1. Create module structure in packages/web-ui/src/: components/, views/, state/, api/
2. Create mod.rs files for each sub-module with re-exports
3. Create api/models.rs with client-side DTO types matching packages/default/src/api/models.rs
4. Create api/client.rs placeholder (HTTP client, implemented in TASK-8.6)
5. Create api/mock.rs placeholder (mock data, implemented in TASK-8.7)
6. Add placeholder components with TODO comments
7. Run: dx build to ensure structure compiles

Expected: Clean module structure, all Dioxus components organized
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 packages/web-ui/src/ module structure created (components/, views/, state/, api/)
- [ ] #2 Client-side API DTO types defined matching server-side DTOs
- [ ] #3 mod.rs files with proper exports
- [ ] #4 dx build succeeds without warnings
- [ ] #5 Existing server/agent/builder bins unaffected (separate crate)
<!-- AC:END -->
