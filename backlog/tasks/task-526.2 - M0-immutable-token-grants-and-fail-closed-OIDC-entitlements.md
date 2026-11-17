---
id: TASK-526.2
title: M0 immutable token grants and fail-closed OIDC entitlements
status: In Progress
assignee: []
created_date: '2026-11-16'
labels: [security, authorization, oidc, agent-access]
parent_task_id: TASK-526
dependencies: []
references:
  - TASK-526
  - docs/knowledge/security/agent-access-tokens-mcp-remediation-context.md
priority: high
---

## Description
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Token authority must remain bounded when POA&M authorization context refreshes. A mutable role refresh must not discard or widen the original token grant. Managed OIDC entitlements must not accumulate removed privileges or authorize requests using stale, missing, or unsuccessfully reconciled authority.

## Goal

Establish M0's server-side authorization groundwork: immutable token-grant context carried through POA&M role refresh, effective-authority intersection with current owner entitlements, and fail-closed managed OIDC reconciliation/freshness with meaningful regression and acceptance tests. This independent slice implements the explicit invariants in this request; exact v1.1 contracts come from the user-provided specification message dated 2026-11-16, available in the main agent's conversation.

## Non-goals

CFAT issuance/storage endpoints, rotation/revocation API, MCP tools/transport, production UI/design changes, service principals, CLI, mutation access, or broad identity refactoring. Do not introduce an Admin gate for Operator, Viewer, or Admin owner self-service.

## Architectural constraints

- The server owns authorization. Separate the original immutable token grant from refreshed owner role/POA&M context; every authorization decision remains bounded by both. Role promotion, context reconstruction, and entitlement refresh cannot widen granted scopes, environment IDs, audience, or expiry.
- Reconcile managed OIDC entitlements from authoritative validated identity data, including removals. Preserve unrelated locally managed grants under existing ownership semantics; do not indiscriminately delete all user roles.
- Commit reconciliation and freshness state atomically. Failed, missing, invalid, or stale managed identity evidence must deny authority that depends on that evidence, without retaining old privileges as a success fallback. A freshness timestamp advances only after successful authoritative reconciliation.
- Reuse existing role mapping, identity-source ownership, session, and clock policy mechanisms. Do not invent a freshness duration or expiry limit absent from v1.1. Implement/test fresh, stale, missing, and failed states using existing policy inputs; if no policy input exists, record the missing design decision before adding a numerical public contract.
- Use forward migrations only if necessary, isolated task-owned database tests, and matching SQLx metadata. Keep public Rust contracts and affected OKF security concepts accurate. The authoritative spec destination remains the parent reference; reconcile implementation with the full source before claiming full M0 completion.

## Verification plan

Use `nix develop --command` for focused existing server authorization unit tests and isolated PostgreSQL reconciliation tests. During preflight, identify the actual test filters and commands from the affected modules and record them in the task. Extend the smallest applicable existing `integration-access` authorization scenario to prove the request boundary; no new top-level integration owner/check. Run its focused gate through `nix build .#checks.x86_64-linux.integration-access.scenarios.<owning-scenario> -L` after resolving the actual registered name. Run `nix build .#okf-knowledge -L` for affected knowledge docs and SQLx metadata checks if query shapes change. Preserve actual output/status; unrun or failed acceptance tests block completion.

Required cases: token grant survives POA&M refresh; owner promotion cannot widen grant; demotion/removal denies removed authority; cross-environment access remains denied; managed OIDC role removal/replacement does not accumulate grants; locally owned grants retain their source semantics; stale/missing/invalid claims or reconciliation failure deny affected access; failed reconciliation does not advance freshness or partially persist privileges; successful fresh reconciliation restores only current permitted authority. Exercise concurrent reconciliation/authorization where persistence permits races.

## Impact, risk, and dependencies

- Impact: server request authorization context, POA&M role refresh, OIDC identity/role mapping and persistence, session-derived authority, tests, and security knowledge contracts.
- Risk: high; privilege escalation, stale access, accidental removal of unrelated grants, or partial reconciliation.
- Dependencies: no implementation prerequisite on TASK-271, token issuance or MCP. TASK-526.1 preserves the complete CF-AGENT-ACCESS-001 v1.1 specification at the authoritative path. This task is now active in its dedicated worktree; legacy OIDC grant ownership/cutover remains a rollout gate, not a missing specification.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tests prove that POA&M role/context refresh preserves the immutable original token grant and that all post-refresh authorization decisions enforce original audience, explicit environment IDs/scopes, expiry, and current owner authority without widening.
- [ ] #2 Tests prove that owner role promotion cannot expand a token grant and that owner demotion or removal immediately denies affected authority at the request boundary.
- [ ] #3 Authoritative managed OIDC reconciliation removes obsolete managed privileges, applies current permitted mappings, and preserves unrelated locally owned grant semantics; positive and removal/replacement tests pass.
- [ ] #4 Freshness is recorded only on successful atomic reconciliation. Stale, missing, invalid, or failed reconciliation denies access dependent on managed OIDC entitlements; rollback and concurrency tests prove no partial grant update or false freshness.
- [ ] #5 Focused component/database tests and the affected existing integration-access authorization scenario pass, including cross-environment denial and all specified failure cases; exact commands and results are recorded.
- [ ] #6 Public authorization contracts and affected OKF docs describe grant immutability, effective-authority intersection, managed/local ownership, freshness, and fail-closed behavior; required documentation/SQLx validation passes. Full v1.1 alignment and acceptance evidence are required before marking M0 complete.
<!-- AC:END -->

## Implementation Notes
<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode on vm-agent in /home/mcamp/code/crystal-forge/TASK-526.2-agent-authorization

User selected implementation on 2026-11-16. Branch `TASK-526.2-agent-authorization` starts at the pushed specification/design checkpoint `f0416e36`, which contains `dev` baseline `bd8d068d` plus the two handoff commits. New worktree verified clean. Integration `dev` has pre-existing untracked locks, task copies and a session file; none will be modified. No `main` worktree exists. Scope is backend authorization groundwork; no browser-visible changes or live database access.

OIDC owner decision: administrator-managed grants remain independent of IdP-managed grants. User has two OIDC accounts, Admin and Operator. Existing grant ownership is ambiguous and MUST NOT be inferred from the Users screenshot, null actor attribution or the displayed `all` environment summary. Preserve existing grants until an explicit classification/cutover decision; unresolved legacy ownership blocks OIDC PAT rollout. Do not claim M0 reconciliation complete while this gate remains. Initial implementation can independently establish immutable authorization-grant semantics and test service refresh.

Verification: focused authorization/POA&M tests using `nix develop`, isolated PostgreSQL only for database tests, existing Access owner scenario where applicable, and `nix build .#okf-knowledge -L`. Record actual commands/results. No milestone is complete yet.

Implementation checkpoint: introduced nonsecret private `RestrictedGrant` fields and explicit `CredentialIdentity` on every PoamActor construction. Four transaction-refreshed readers require mcp audience, both CVE/POA&M read scopes, future expiry and current-owner/original-grant intersection. Restricted actors never inherit Admin wildcard or mutation flags. Unsafe shared-plan reads fail closed; other readers, exports and every restricted mutation are denied until explicit capability/admission policy exists. Browser authentication behavior is unchanged. No token secret, API endpoint, schema migration or OIDC assignment change is included.

Before the user's build-farm-only instruction, focused authorization unit tests passed 2/2 and `cargo check --tests` passed. Subsequent SQLx test/public-contract refinements have not been locally compiled. From that instruction onward, use `nix build --max-jobs 0` only, not local Cargo. Read-only review identified missing Admin-owner mutation/hydration tests and public error contracts; those have been added. `git diff --check` passes. Farm execution and final docs validation pending; no acceptance criterion is marked complete.

Farm checkpoint: `nix build .#okf-knowledge --max-jobs 0 --no-link -L` passed (58 validator tests, 198 concepts, 219 Mermaid blocks). Focused PostgreSQL proof uses a temporary override of the existing core derivation, on the farm only, with a fresh sandbox-owned Unix-socket-only cluster and verified data_directory; no host database is contacted. The first attempt timed out; a transient Nix stale-file-handle evaluation was retried. The next attempt failed because the temporary harness inherited core test selection and exposed an empty database to online SQLx macros. PostgreSQL stopped cleanly. The harness is corrected to use SQLX_OFFLINE and select the exact integration test; the regression itself is not yet claimed passed.

SQLx metadata disposition: no schema or compile-time query-macro changes; the added projection guard uses runtime-bound SQL. No offline metadata regeneration is required for this checkpoint. Token-row validation, fresh OIDC persistence, request-boundary and concurrent admission evidence remain outstanding.
<!-- SECTION:NOTES:END -->
