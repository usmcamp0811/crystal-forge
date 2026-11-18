---
id: TASK-531
title: Recheck the acting Admin inside every admin mutation transaction
status: Backlog
assignee: []
created_date: '2026-11-17'
labels: [security, authorization]
dependencies: []
references:
  - TASK-529.2
priority: medium
---

## Description
<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-529.2 (MR !345, Draft) rechecks the acting Admin inside the transaction of
`PATCH /api/v1/admin/users/:id` only. `require_admin_session` returns the actor
and session ids without a lock. `patch_admin_user` then takes the grant advisory
lock, locks the actor and target user rows by ascending id, and verifies that the
actor is active, holds an Admin role assignment and owns a live session before it
looks up the target or writes. A failed recheck returns 403 with no write and no
audit event.

The other administrator mutations in
`packages/default/crates/cf-server/src/handlers/api/admin.rs` still authorize with
`require_admin_user` before their work starts. A demoted, disabled or logged-out
Admin whose in-flight request already passed that check can still commit:

- `create_user` (`POST /api/v1/admin/users`): inserts the user, password, role,
  environment memberships and audit events as separate pool statements. It is
  not one transaction.
- `delete_user` (`DELETE /api/v1/admin/users/:id`): `delete_user_with_admin_guard`
  takes the advisory lock and the target row but does not recheck the actor. The
  audit event is written after commit.
- `upsert_oidc_mapping` and `delete_oidc_mapping`
  (`/api/v1/admin/oidc-mappings`): pool statements followed by a separate audit
  write.
- `update_classification_config` and `update_automatic_retry_policy`
  (`PUT /api/v1/admin/classification-config` and
  `/api/v1/admin/automatic-retry-policy`): one pool statement each after the early
  check, with no audit event.

`disable_user_with_admin_guard` in `queries/admin.rs` has no HTTP handler in the
current tree; standalone disable is only the PATCH `enabled` field. No separate
environment membership admin route exists outside create user and PATCH. Confirm
this list against the handlers when the task is groomed, because routes may change
before then.

Desired outcome: each listed mutation re-authorizes the acting Admin (active, Admin
role assignment, live session) inside its own transaction, after its
synchronization locks and before it reads the target or writes. A stale actor
receives 403 with no write and no audit event, and cannot learn whether a target
exists. Mutations that today span several pool statements and must be atomic with
that check need one transaction. Reuse the PATCH design (`require_admin_session`
plus the `patch_admin_user` lock order: advisory lock, actor and target user rows
by ascending id, actor recheck) instead of adding a new mechanism. Add regression
coverage for a demoted, disabled and logged-out actor per mutation.

Non-goals:

- Changing the PATCH recheck, which TASK-529.2 owns.
- Read-only admin endpoints, and non-admin role checks (Operator, Viewer).
- A new authorization model, session scheme or schema migration unless the work
  proves one is required.
- PAT, MCP or token-issuance work.

References: TASK-529.2; PATCH design in
`docs/knowledge/security/oidc-entitlement-ownership.md` and the User PATCH
contract in `docs/knowledge/api/builders-queues-environments-dashboard-admin-api.md`.

This is a Backlog capture. It is not sprint-ready: it has no acceptance criteria,
verification plan or risk assessment yet.
<!-- SECTION:DESCRIPTION:END -->
