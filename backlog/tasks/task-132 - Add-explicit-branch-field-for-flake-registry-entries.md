---
id: TASK-132
title: Add explicit branch field for flake registry entries
status: To Do
assignee: []
created_date: '2026-02-26 21:57'
updated_date: '2026-02-26 21:58'
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem Statement:
Flake sync currently infers branch from the remote default branch at runtime. This is non-deterministic for repos that intentionally track a non-default branch (for example dev, release/*, or pinned operations branches).

Goal:
Add explicit branch support for flake registry entries so sync behavior is deterministic and user-controlled, while still providing auto-detect convenience during create/edit flows.

Non-Goals:
- Implement credentials management for private repositories (tracked separately in TASK-131).
- Add support for per-flake subdirectory/path selection.
- Change global auth model or RBAC behavior.

Architectural Constraints:
- Preserve API/domain/infrastructure/UI separation in existing repository patterns.
- Keep UI free of branch sync business logic; branch selection is data entry + API call only.
- Keep changes scoped to flake registry model, handlers, queries, and flakes UI flow.

Verification Plan:
- nix develop -c cargo check (packages/default)
- nix develop -c cargo check (packages/web-ui)
- nix develop -c cargo test handlers::api::flakes::tests (or equivalent targeted tests)
- nix develop -c cargo fmt -- --check (touched crates)

Impact Areas:
- Backend API models and handlers for flakes create/edit/sync
- Flake DB queries and SQLx metadata if query shapes change
- Flakes view add/edit modal fields and API client payloads

Risk Level:
Medium: API/schema contract and sync semantics change for flakes, but scope is contained to flake registry flows.

Dependencies:
- None external; builds on TASK-130 behavior and existing flake sync endpoints.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Creating a flake accepts optional branch input; when omitted, backend auto-detects and persists remote default branch.
- [ ] #2 Editing a flake allows changing branch and repo URL; changes persist across navigation/reload.
- [ ] #3 Sync all and sync single flake use persisted branch for each flake, not hard-coded defaults.
- [ ] #4 Validation errors are returned for unreachable repo URLs or invalid branch inputs with user-visible messages.
- [ ] #5 Flakes UI clearly shows branch field in add/edit flow and preserves existing UX quality.
- [ ] #6 Targeted verification commands pass for touched backend and web-ui crates.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add branch field to flake data model and persistence/query layer.
2. Update create/edit API payloads and handlers to validate + persist branch (with auto-detect fallback).
3. Update sync handlers to consume persisted branch per flake.
4. Update web-ui add/edit flake flows and API client DTOs for branch.
5. Run targeted verification and update task notes with outcomes.
<!-- SECTION:PLAN:END -->
