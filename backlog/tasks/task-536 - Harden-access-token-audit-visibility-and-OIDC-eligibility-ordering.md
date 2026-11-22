---
id: TASK-536
title: Harden access-token audit visibility and OIDC eligibility ordering
status: Backlog
assignee: []
created_date: '2026-11-19'
labels: [security, agent-access]
dependencies: []
references:
  - TASK-534.3
priority: medium
---

## Description
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

A security review of TASK-534.3 (owner access-token service) found gaps that the
first slice documents but does not fix:

1. **Audit window flooding.** `GET /api/v1/admin/audit-events` returns only the
   newest 300 rows and applies filters after that limit. A Viewer can write about
   20 token events per hour (10 creates and 10 revokes inside the live-token cap).
   Repeated cycling, or several accounts, can push administrator events such as
   `user_deleted` out of the visible Audit Log.
2. **OIDC eligibility ordering.** Create and rotate check OIDC entitlement
   freshness with a pool query before the owner transaction takes its locks. A
   concurrent sign-in reconciliation can change entitlements between that check
   and the token commit. M0 intersects the grant with current owner state at use
   time, so the effect is bounded, but spec section 4.4 asks for the recheck under
   locks. `queries::oidc_entitlements::require_fresh_entitlements` takes a pool
   and needs a transaction-capable variant.
3. **Session validity inside the transaction.** The token handlers drop the
   session id after authentication, so a create that passed authentication just
   before "revoke all sessions" can still commit. The admin PATCH path already
   passes the session id into its transaction.
4. **Smaller items.** Names accept Unicode format characters (for example
   U+202E). Token audit events record the user UUID as `actor_identifier` and no
   `request_origin`, unlike other events. `validate_presented_token` compares
   expiry with the application clock while status uses database time. Responses
   from extractor rejections and router 404/405 lack `Cache-Control: no-store`.
   There is no `Retry-After` on 429 and no request id in error bodies.

## Desired outcome

Each item is either fixed with a test or closed with a written decision.
Administrator events stay visible regardless of volume of self-service events.

## Notes

Documented as known limits in
`docs/knowledge/security/access-token-service.md`.
<!-- SECTION:DESCRIPTION:END -->
