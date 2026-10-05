---
id: TASK-479
title: Audit CSRF enforcement on mutating API handlers
status: Backlog
assignee: []
created_date: '2026-10-05 01:43'
labels:
  - security
  - api
  - csrf
dependencies: []
references:
  - docs/knowledge/meta/cleanup-record.md
  - packages/default/crates/cf-server/src/handlers/api/auth_session.rs
  - packages/default/crates/cf-server/src/auth/extractors.rs
priority: medium
type: spike
ordinal: 500000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found during the TASK-478 documentation review at commit 3b23d36f.

CSRF enforcement is per handler. A handler enforces the double-submit `x-csrf-token` check only when it calls `require_csrf` (handlers/api/auth_session.rs). The guard extractors `RequireOperator` and `RequireAdmin` (auth/extractors.rs) do not check CSRF, and no router layer does. Handlers in `compliance.rs`, `systems.rs`, `user_notifications.rs`, `user_sessions.rs`, `framework_requirements.rs`, `commits.rs`, `builders.rs`, and `poam.rs` call it. Handlers such as `flakes::create_flake` (RequireOperator) and most of `environments.rs`, `admin.rs`, `caches.rs`, and `deployment_policies.rs` have no call.

The session cookie uses SameSite=Lax, which limits cross-site requests but is not the intended control. This task decides, per mutating route, whether the check is required, and records the decision.

Out of scope: any exploit work. Treat the output as an audit with a fix plan.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 List every mutating route in bin/server.rs with its authorization pattern (extractor or rbac helper) and whether it calls require_csrf.
- [ ] #2 Decide whether enforcement should move to a shared layer or extractor, or stay per handler, and record the decision with its rationale.
- [ ] #3 File follow-up implementation tasks for each route that needs the check; do not change handlers in this task.
- [ ] #4 Update docs/knowledge/security/session-cookies-and-csrf.md and docs/knowledge/api/api-overview-errors-and-streaming.md with the decided policy.
<!-- AC:END -->
