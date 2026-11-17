---
type: Security Model
title: OIDC entitlement ownership and freshness
description: Explains local and IdP grant union, legacy classification, ordered reconciliation, future PAT freshness eligibility, and the one-time upgrade procedure.
tags:
  - crystal-forge
  - security
  - oidc
  - authorization
implementation_status: partial
sources:
  - id: migration
    resource: packages/default/crates/cf-server/migrations/0335_oidc_entitlement_sources.sql
    title: Source flags and entitlement state migration
  - id: reconciliation
    resource: packages/default/crates/cf-server/src/queries/oidc_entitlements.rs
    title: Ordered reconciliation and freshness eligibility helper
  - id: admin
    resource: packages/default/crates/cf-server/src/queries/admin.rs
    title: Source-aware administrator grant replacement and guards
  - id: callback
    resource: packages/default/crates/cf-server/src/handlers/api/auth_oidc.rs
    title: Login generation reservation and bound callback reconciliation
  - id: policy
    resource: packages/default/crates/cf-config/src/config/oidc.rs
    title: Positive entitlement maximum age configuration
---

# OIDC entitlement ownership and freshness

## Status and authority

TASK-529.2 implements the source model and callback reconciliation described
below in this worktree. Current-source farm verification remains pending.
`implementation_status: partial` also records that the freshness helper is not
wired to PAT admission. There are no PAT/MCP endpoints or token-row validator.
The approved [CF-AGENT-ACCESS-001 v1.1 specification](agent-access-tokens-mcp-remediation-context.md),
sections 4.4 and 5.3, defines the release requirements; its baseline description
of preserve-on-no-mapping behavior is historical.

The owner has two existing OIDC accounts, Admin and Operator, and chose
independent local overrides. Neither that account list nor the Users page's
source labels or `all` environment summary proves ownership of any grant.

## Effective grants are a union

Migration `0335_oidc_entitlement_sources.sql` adds these fields to both
`user_role_assignments` and `user_environment_memberships`:

| Field | Contribution |
| --- | --- |
| `local_grant` boolean | Independent administrator/local grant. |
| `oidc_identity_id` nullable UUID | Managed grant from one bound OIDC identity. |
| `legacy_ownership_unresolved` boolean | Preserved effective grant with unknown ownership. |

Each effective row represents the union of its contributions. Local and IdP
ownership can overlap on the same role or environment row. Removing one source
retains the row if another source remains. The source CHECK requires
`local_grant OR oidc_identity_id IS NOT NULL OR legacy_ownership_unresolved`.
A composite foreign key binds the identity pointer to the same user. Identity
deletion cannot silently convert managed grants to local grants.

The migration preserves existing effective rows, IDs and actor attribution.
For users with any external identity, it sets existing grants to unresolved
legacy ownership, not inferred IdP ownership. Grants for users without external
identities remain local. Null actor attribution is not classification evidence.

Explicit admin role replacement clears local/unresolved **role** contributions
and installs the requested local role, while retaining IdP contributions.
Explicit environment replacement does the same for **memberships**, including
an explicitly empty list. Omitted categories stay unchanged. A role-only change
does not resolve legacy memberships. Reassigning the same effective value still
classifies that category as local. PATCH and list responses report the effective
union, so requested Viewer can still return Operator or Admin from the IdP.
The handler commits categories separately; do not assume a multi-field PATCH is
one atomic cutover. Verify each result before continuing.

## Ordered callback reconciliation

Login initiation reserves a positive database sequence generation **before**
redirect. The server retains it with state, nonce and PKCE verifier in the
server-side flow store. The callback must reuse that generation; it cannot
allocate a new one. The noncycling sequence uses `CACHE 1`; abandoned flows
leave intentional gaps. Flow state is in memory with a default 600-second TTL,
so server restart or a different instance can lose the flow. This TTL is
separate from entitlement age and browser session expiry.

The callback validates the token, nonce and verified email, then resolves the
identity by provider `oidc`, subject and issuer, not email. Reconciliation:

1. Locks the user and registers a durable `pending` attempt only when its
   generation is strictly greater than the identity's highest accepted generation.
   Rejected older success or failure evidence cannot change claims, grants,
   freshness or state, and cannot restore removed rights.
2. Acquires transaction advisory lock `(526, 2)` before the user lock, then locks
   the identity. The attempt UUID must still match. Guarded admin role changes,
   disable and deletion use the same advisory lock. Membership replacement uses
   the user lock. Pending registration and cleanup use the user lock alone
   because they do not change roles.
3. Requires an active bound user and exactly one linked external identity,
   counting **all** providers. Multi-identity linkage is unsupported and fails
   closed for login as well as future PAT eligibility, even for legacy users.
4. If either grant category remains unresolved, returns `legacy_unresolved`.
   Existing grants and browser login survive even missing or malformed groups,
   but successful freshness does not advance. Other binding, activity, ordering
   and linkage checks still apply; this is not universal legacy-login support.
5. Otherwise requires authoritative group evidence. Missing/null/malformed claims
   or nonstring array elements deny managed login. Valid empty arrays or blank
   strings are authoritative empty evidence. Dotted claim paths are supported.
6. Locks mappings and resolves active environments. Unknown/inactive mapped
   environments fail reconciliation. Success replaces only this identity's IdP
   contribution, stores claims, sets `valid` and stamps
   `last_successful_entitlement_sync_at` with database `clock_timestamp()` in one
   transaction. Local overlap survives.

The highest mapped role is Admin, then Operator, otherwise Viewer. Valid empty
groups or no matching mapping produce an **OIDC-owned Viewer and no managed
environment memberships**, removing previous managed privileges. Independent
local grants can still produce a higher effective role or environment access.

Failure rolls back the grant/claims/successful-time transaction. Attempt-fenced
cleanup marks only the current pending attempt `invalid`. Cancellation or cleanup
failure leaves durable `pending`; neither state is fresh. An older failure cannot
invalidate a newer success. Errors before the callback reaches reconciliation
deny that login but do not register a pending entitlement attempt.

The last-Admin guard serializes administrative decisions with OIDC revocation.
It **must not retain revoked IdP Admin authority**. Removing the last IdP Admin
can leave zero enabled Admins. An independent recovery administrator is a cutover
prerequisite, not a reason to keep revoked privileges.

## Freshness policy and future PAT boundary

`require_fresh_entitlements` currently checks eligibility only. It requires:

- an active owner and the requested identity bound to that owner with provider `oidc`;
- exactly one linked external identity and no unresolved legacy grants in either category;
- state `valid` and a successful timestamp no later than database
  `statement_timestamp()` and strictly newer than database time minus maximum age.

Missing, pending, invalid, unresolved, future-dated and age-equal timestamps deny.
Configuration defaults to 86,400 seconds (24 hours). Use
`services.crystal-forge.server.oidc.entitlementMaxAgeSeconds`, which exports
`CRYSTAL_FORGE_OIDC_ENTITLEMENT_MAX_AGE_SECONDS`; accepted values are integers
1 through 4,294,967,295 seconds. Zero cannot disable the policy.

This age bounds **future OIDC-owned PAT admission**, not existing browser session
TTL. Successful managed sign-in refreshes it. No background provider refresh is
implemented here. A future unattended PAT can pause until renewed sign-in or
supported sync. Future handlers must wire this helper and validate the token row,
owner binding, revocation, grant and resource authority under appropriate
serialization. The helper alone is not an admission transaction.

## One-time upgrade procedure

This is an operator procedure, not an executed migration or database session.

1. Before deployment, take and verify a restorable backup. Verify the configured
   issuer, subject-to-user bindings, linked-identity counts, claim path, group
   mappings and active environment names. Confirm an independent, reachable
   local/bootstrap recovery Admin path that works in the selected authentication
   mode. A bootstrap **group mapping** is still IdP-managed and alone does not
   protect against losing the final IdP Admin. Do not proceed without recovery.
2. Inventory the two existing accounts and every role/membership separately.
   Record current effective grants, identity bindings, source flags and the
   intended local or IdP ownership. Do not derive ownership from screenshots,
   actor nulls or current group membership alone. Rehearse on an isolated restored
   copy before scheduling the production change.
3. Deploy the forward migration through the established upgrade process. Verify
   preserved effective grants and unresolved flags before classification. Leave
   ambiguous grants unresolved until their owner approves the inventory; they
   continue to block future OIDC PAT eligibility.
4. For the selected independent local overrides, explicitly reassign approved
   roles and memberships through existing admin controls, one category at a
   time. Verify the effective result and audit record after each operation.
   **This creates LOCAL overrides. IdP group removal will not remove them.**
   Supply an explicit membership list (including empty when intended); a role
   reassignment alone does not classify memberships.
5. If a pre-existing grant must instead become IdP-owned, stop for an explicit,
   controlled source-classification change reviewed against the inventory,
   identity binding, source constraints and locking contract. Existing admin UI
   replacement does not offer that operation. This slice supplies no
   classification API or CLI. Do not mistake reassignment for IdP cutover.
6. After both categories are resolved, perform a fresh sign-in and verify current
   mappings, managed removals, surviving local overlap and successful sync state.
   Test recovery and group removal first in the isolated rehearsal. Verify that
   unresolved/failed attempts do not become fresh. Keep the backup and reviewed
   inventory for recovery; do not bypass failure by stamping freshness manually.

There is no automatic plaintext token retrieval or Admin human impersonation
path. The approved future PAT design returns a secret once to its authenticated
owner; this groundwork does not issue tokens or let an Admin mint another
human's session/token.

## Pending verification

Source files and regression definitions are evidence of implementation, not of
passing execution. Current-source component/database, signed OIDC callback,
concurrency, upgrade and request-boundary acceptance remain to be demonstrated.
The existing Access scenario proves only its asserted browser-admin boundary;
it does not prove PAT admission or a signed provider callback.

When execution is authorized, use the configured farm (`max-jobs = 0`, builders
from `/etc/nix/machines`), from the task worktree:

```bash
nix config show | grep -E '^(builders|max-jobs) '
nix build .#packages.x86_64-linux.default.cf-server-core-drv --no-link -L
nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap --no-link -L
nix build .#okf-knowledge --no-link -L
```

These commands were not run for this documentation update. The core package's
ordinary tests do not execute ignored PostgreSQL integration targets.
`tests/oidc_entitlements.rs`, `tests/admin_source_grants.rs` and the POA&M target
are selected by the existing `packages/devScripts/tests/database-regressions.sh`;
their execution needs a farm/CI-owned disposable database and retained results.
Do not substitute local Cargo or a live database. M0 remains incomplete until
the [credential grant release gates](credential-grant-groundwork.md#remaining-release-gates)
and approved specification acceptance evidence are satisfied.

## Related concepts

- [OIDC role mapping](oidc-role-mapping.md)
- [API authentication and authorization](api-authentication-and-authorization.md)
- [Authentication configuration](../operations/authentication-modes-and-oidc-configuration.md)
- [Session cookies and CSRF](session-cookies-and-csrf.md)
