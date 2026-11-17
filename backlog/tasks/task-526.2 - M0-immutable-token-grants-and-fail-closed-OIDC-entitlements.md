---
id: TASK-526.2
title: M0 immutable token grants and fail-closed OIDC entitlements
status: To Do
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
- Dependencies: no implementation prerequisite on TASK-271, token issuance, MCP, or design integration. Explicit request requirements are sufficient for this invariant-only groundwork. Use the complete user-provided CF-AGENT-ACCESS-001 v1.1 message dated 2026-11-16 for constants and contracts; TASK-526.1 preserves it at the authoritative spec path. There is no missing-source blocker. Implementation requires normal task preflight and a dedicated worktree; this task remains To Do without an active lock.
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
