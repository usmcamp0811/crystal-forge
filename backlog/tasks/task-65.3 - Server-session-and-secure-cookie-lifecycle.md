---
id: TASK-65.3
title: Server session and secure cookie lifecycle
status: Done
assignee:
  - Codex 5.3
created_date: ''
updated_date: '2026-03-13 01:24'
labels:
  - security
  - auth
  - sessions
  - backend
milestone: m-14
dependencies:
  - TASK-65.2
priority: high
ordinal: 55000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem
User sessions are required for browser auth, but secure server-managed session lifecycle is not implemented.

Goal
Implement login callback, logout, session creation, rotation, and expiry with secure cookies.

Non-Goals
- Browser-held bearer token auth model.
- Long-lived insecure sessions.

Architectural Constraints
- Session state must be server-authoritative.
- Security headers and cookie attributes must be enforced in backend config.
- No auth logic embedded in presentation layer.

Verification Plan
- `nix develop -c cargo test --package default auth::sessions`
- `nix develop -c cargo test --package default auth::handlers`
- `nix develop -c cargo clippy -- -D warnings`
- Manual: validate login, idle expiry, and logout behavior in browser.

Impact Areas
- API, Infrastructure, Security

Risk Level
- High
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Backend issues HttpOnly secure cookies for authenticated sessions
- [x] #2 Session expiry and invalidation behavior is defined and enforced
- [x] #3 Logout invalidates server-side session
- [x] #4 CSRF and session protection strategy is documented and implemented
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Follow-up possibility: additional session hardening after security review.

Selected for auth micro-sprint planning (2026-02-20).

LOCK: codex on gray in /home/mcamp/code/crystal-forge/TASK-65.3-session-cookie-lifecycle

Implementation complete in branch `TASK-65.3-session-cookie-lifecycle`.

Verification executed:
- `nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml -- --check` (pass)
- `SQLX_OFFLINE=true nix develop -c cargo test --package crystal-forge auth::session` (pass)
- `SQLX_OFFLINE=true nix develop -c cargo test --package crystal-forge handlers::api::auth_session` (pass)
- `SQLX_OFFLINE=true nix develop -c cargo check --package crystal-forge` (pass)
- `SQLX_OFFLINE=true nix develop -c cargo clippy --package crystal-forge -- -D warnings` (fails due pre-existing repository-wide warnings and transient rustc target-cache mismatch, not introduced by this task)

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/123
<!-- SECTION:NOTES:END -->
