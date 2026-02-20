---
id: TASK-65.0
title: Developer auth mode with local selector screen
status: To Do
assignee:
  - Codex 5.3
created_date: ''
updated_date: '2026-02-20 04:22'
labels:
  - security
  - auth
  - devex
  - ui
  - api
milestone: m-14
dependencies:
  - TASK-65
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem
Local development should not depend on external OIDC provider setup, otherwise development velocity is degraded.

Goal
Provide a development-only auth mode with a local selector login screen for Admin, Operator, and Viewer, and default this mode in devshell.

Non-Goals
- Production fallback auth bypass.
- Header or query impersonation shortcuts.
- Changes to agent key-auth flow.

Architectural Constraints
- Dev mode must converge into the same authorization path as OIDC after identity establishment.
- Environment guardrails must be enforced server-side, not UI-only.
- No hidden global auth bypass toggles.

Verification Plan
- `nix develop -c cargo test --package default auth::dev_mode`
- `nix develop -c cargo test --package web-ui auth_dev_selector`
- `nix develop -c cargo clippy -- -D warnings`
- Manual: selector login for all three roles; reject `AUTH_MODE=dev` outside dev profile.

Impact Areas
- UI, API, Domain, Infrastructure

Risk Level
- High
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Local selector login screen exists in `AUTH_MODE=dev`
- [ ] #2 Selector supports Admin, Operator, and Viewer identities
- [ ] #3 Devshell defaults to `AUTH_MODE=dev`
- [ ] #4 Application startup fails when `AUTH_MODE=dev` is used outside development profile
- [ ] #5 UI clearly indicates dev auth mode is active
- [ ] #6 `/api/agent/**` routes remain key-auth based and unaffected
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Follow-up possibility: per-role dev fixture customization.

Selected for auth micro-sprint planning (2026-02-20).
<!-- SECTION:NOTES:END -->
