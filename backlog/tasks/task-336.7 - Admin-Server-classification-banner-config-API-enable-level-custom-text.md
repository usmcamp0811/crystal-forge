---
id: TASK-336.7
title: 'Admin Server: classification banner config API (enable/level/custom text)'
status: Review
assignee: []
created_date: '2026-06-20 02:59'
updated_date: '2026-06-20 18:45'
labels:
  - admin
  - server
  - classification
  - api
  - backend
milestone: 'm-20: Design Parity Missing Surfaces'
dependencies:
  - TASK-336.2
references:
  - TASK-336.2
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/284'
modified_files:
  - packages/default/migrations/0142_classification_banner_config.sql
  - packages/default/src/api/models.rs
  - packages/default/src/bin/server.rs
  - packages/default/src/handlers/api/admin.rs
  - packages/default/src/queries/admin.rs
  - packages/web-ui/src/api/client.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/components/layout/app_shell.rs
  - packages/web-ui/src/state/app_state.rs
  - packages/web-ui/src/views/admin.rs
parent_task_id: TASK-336
priority: medium
ordinal: 313000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
The Admin Server tab Classification banners card allows enabling/disabling DoD/CNSS classification markings, selecting a classification level, and entering custom marking text, but the current implementation is session-local only and does not persist or render globally after leaving the Server config view.

## Goal
Persist classification banner configuration in the database, expose it through backend API endpoints, wire the Admin Server card to real read/save behavior, and render the classification banner at the top and bottom of the UI whenever enabled.

## Explicit Non-Goals
- Do not implement unrelated server settings such as heartbeat persistence, background jobs, server info, audit export, database backup, reload config, or session invalidation unless a schema/table addition is directly required for shared server settings storage.
- Do not fake persistence with local storage or in-memory-only state.
- Do not redesign global shell/sidebar/topbar beyond adding the classification banner frame.
- Do not implement role/permission changes unrelated to the classification config endpoint.

## Scope
1. Add database persistence for classification banner config.
2. Add backend API GET/PUT support for enabled, level, and custom text.
3. Add/update API models and web-ui client functions.
4. Wire the Admin Server Classification banners card to load and save real config.
5. Render top and bottom UI banners globally when enabled.
6. Keep unsupported server settings out of scope and tracked by existing follow-up tasks.

## Architectural Constraints
- UI components should remain presentation/composition focused.
- Backend DTOs should mirror API/server models.
- Database schema changes must include a migration.
- SQLx metadata must be refreshed with the repository devshell and local process-compose database workflow.
- No business logic in views beyond simple composition/state wiring.

## Impact Areas
- packages/default/migrations/**
- packages/default/src/handlers/api/**
- packages/default/src/queries/**
- packages/default/src/api/models.rs
- packages/web-ui/src/api/**
- packages/web-ui/src/state/** or app shell components
- packages/web-ui/src/views/admin.rs

## Risk Level
Medium. This touches global app rendering and persisted admin/server configuration. Main risks are incorrectly displaying stale classification markings, failing SQLx sync, or mixing session-local preview state with persisted global state.

## Dependencies
- TASK-336.2 Admin view parity branch is used as the implementation base per maintainer instruction.
- Existing follow-up tasks cover other server settings: TASK-336.3, TASK-336.5, TASK-336.6, TASK-336.8.

## Verification Plan
- Run `nix develop -c bash -lc 'cd packages/web-ui && cargo fmt -- --check && cargo check --target wasm32-unknown-unknown'`.
- Run `nix develop -c bash -lc 'cargo check --manifest-path packages/default/Cargo.toml'` after database/schema setup.
- Start local process-compose DB and run `sqlx database reset -y && cargo sqlx prepare` from `packages/default` using the verified local dev database.
- If feasible, run targeted backend tests for the admin classification config handler/query.
- Manually inspect or describe how to verify that the banner persists after navigating away from Admin Server.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Backend exposes GET and PUT for classification config (enabled, level, custom text)
- [x] #2 The Admin Server Classification banners card reads and saves configuration via the real API
- [x] #3 Classification banner renders at top and bottom of the UI when enabled and remains visible after navigating away from the Server config view
- [x] #4 Classification config is persisted in the database through a migration and SQLx metadata is refreshed
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
All four acceptance criteria met in MR !284 commit 1d7c404b. Migration 0142 creates classification_banner_config table seeded to disabled/UNCLASSIFIED. Backend GET/PUT /api/v1/admin/classification-config endpoints added (write requires admin). AppShell fetches config on mount and renders ClassificationBar (position:fixed top+bottom) for all authenticated routes whenever enabled. Admin Server card reads from AppState and saves via real API, propagating changes to AppState immediately. SQLx prepare passed with all 142 migrations applied.
<!-- SECTION:FINAL_SUMMARY:END -->
