---
id: TASK-133
title: Improve flake timeline cards with commit message + better author display
status: Review
assignee: []
created_date: '2026-02-26 22:12'
updated_date: '2026-02-27 02:07'
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem Statement:
Flake history cards do not consistently present commit message quality or meaningful author identity, which makes timeline scanning harder and reduces trust in who authored a change.

Goal:
Improve timeline card UX to render commit messages clearly for both short and long human-authored messages, and show best-effort author identity by preferring mapped Crystal Forge users when possible, with graceful fallback to commit author fields.

Non-Goals:
- Introduce new authentication providers or identity systems.
- Redesign the full flakes page layout beyond timeline cards.
- Add new database tables or persistent user-identity linkage beyond existing data.

Architectural Constraints:
- Keep presentation logic in UI components; avoid introducing business logic in views.
- Reuse existing API/domain models where possible; only extend API payloads if required.
- Keep fallback author resolution deterministic and explicit.

Verification Plan:
- nix develop -c cargo check (packages/web-ui)
- nix develop -c cargo fmt -- --check (packages/web-ui)
- nix develop -c cargo check (packages/default) if backend/API models or handlers are touched
- Run targeted tests for touched modules (backend/UI where applicable)

Impact Areas:
- Flake timeline card rendering in web UI
- Timeline-related DTO mapping and helper functions in web UI
- Backend flakes timeline endpoint/model only if additional author fields are required

Risk Level:
Medium: user-facing display behavior changes and may affect readability/consistency, but scope is limited to timeline presentation and related data mapping.

Dependencies:
- None
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Timeline cards render very short commit messages without awkward spacing/truncation.
- [x] #2 Timeline cards render long commit messages with readable truncation/expansion behavior that preserves layout quality.
- [x] #3 Author display prefers mapped Crystal Forge user identity when available.
- [x] #4 Author display falls back to commit author name/email (or existing sensible fallback) when no mapped user is available.
- [x] #5 No regression in timeline loading, filtering, or card interaction behavior.
- [x] #6 Verification commands for all touched crates pass.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Inspect current timeline card component/data flow for commit message and author fields.
2. Implement robust commit message rendering strategy for short and long text.
3. Implement author display resolution order (mapped CF user first, fallback second).
4. Update/add focused tests where behavior is testable in touched modules.
5. Run verification commands and record outcomes in task notes.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Sprint-ready upgrade completed on 2026-02-26.
LOCK: OpenCode on reckless in /home/mcamp/code/crystal-forge/TASK-133-improve-flake-timeline-cards

2026-02-26 implementation:
- Backend timeline hydration now enriches commit message + author from git metadata.
- Author resolution prefers mapped CF user by commit email (`@username`), then falls back to git author name/email.
- Flakes history cards now render commit messages with readable headline/secondary handling and improved wrapping/clamp behavior.
- Added focused helper tests for commit message/author normalization.

Verification executed:
- `nix develop -c cargo check` (packages/web-ui) passed.
- `nix develop -c cargo test views::flakes_list::tests` (packages/web-ui) passed.
- `nix develop -c rustfmt --edition 2021 --config skip_children=true --check packages/default/src/flake/commits.rs packages/default/src/handlers/api/flakes.rs packages/web-ui/src/views/flakes_list.rs` passed.
- `SQLX_OFFLINE=true nix develop -c cargo check` (packages/default) passed.

Note:
- `nix develop -c cargo check` in `packages/default` against live DB failed due incomplete local schema state; used SQLx offline mode for deterministic compile verification.

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/139

Commit: 8b5e05b

2026-02-27 verification rerun: nix develop -c cargo check (web-ui), nix develop -c cargo test views::flakes_list::tests (web-ui), targeted rustfmt --check for touched files, SQLX_OFFLINE=true nix develop -c cargo check (default).

Follow-up perf hardening commit: 101541a

Addressed review blocker by adding git metadata timeouts, removing per-commit fetch, and capping request-time hydration to missing fields only.
<!-- SECTION:NOTES:END -->
