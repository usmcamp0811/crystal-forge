---
type: Security Model
title: Credential grant boundaries in POA&M services
description: Explains the partial M0 credential-grant boundary, restricted read intersection, disabled mutation paths, and remaining PAT and OIDC admission gates.
tags:
  - crystal-forge
  - authorization
  - access-tokens
implementation_status: partial
---

# Credential grant boundaries in POA&M services

## Status and authority

TASK-526.2 starts the authorization prerequisite for
[CF-AGENT-ACCESS-001 v1.1](agent-access-tokens-mcp-remediation-context.md).
This change adds domain authorization inputs; it does **not** implement PAT
authentication, token persistence, lifecycle endpoints, or MCP transport.
No HTTP endpoint accepts these inputs from a client. The existing browser
session extractors and routes remain the only human authentication channel.

The implementation lives in `cf-server/src/auth/authorization.rs` and
`cf-server/src/services/poam.rs`. `CredentialIdentity` distinguishes an explicitly
authenticated browser session from a `RestrictedGrant`. Grant fields are
private and contain only public token ID, audience, scopes, environment IDs and
expiry. The constructor does not validate a bearer secret or prove revocation
state. A future admission service must validate those facts independently.

## Refresh invariant

POA&M read refresh reloads the active owner, current roles and memberships. It
preserves the original credential grant instead of deriving unrestricted
authority from the owner again.

- Browser sessions keep their existing role and Admin environment behavior.
- Restricted credentials must have audience `mcp`, every required scope and an
  expiry strictly later than current server time.
- For non-Admin owners, effective environments are the intersection of original
  credential environment IDs and current owner memberships.
- For Admin owners, the credential's original explicit IDs still bound access;
  an Admin role never becomes a credential wildcard.
- An empty effective set grants no environment-scoped access.
- Refreshed restricted actors have no Admin bypass and no mutation permission.
  Stale request-carried `is_admin` or `can_mutate` flags cannot restore them.

## Supported and denied domain paths

The four refreshed CVE/POA&M relationship readers return both CVE and POA&M
metadata. They therefore require **both** `cves:read` and `poams:read`:

- `system_cve_triage_detail`
- `fleet_cve_batch_detail`
- `cve_relationships`
- `cve_relationships_for_rows` and its transaction-aware helper

The server authorizes the selected system in the refreshed scope. Hidden and
absent systems use the existing not-found behavior. If a related plan has hidden
current or recorded subjects, the restricted reader denies the whole response
with not found rather than disclose unprojected plan text. This is a fail-closed
boundary, not an implemented scoped POA&M projection.

Other public POA&M read paths and both register export reconstruction paths
require an explicit browser credential until their MCP-safe representations
exist. All restricted mutation paths are denied, even when a grant contains
`poams:write` and its owner is Admin or Operator. Generic progress permission
does not authorize triage, risk acceptance, verification or closure.

## OIDC ownership decision and rollout gate

The owner selected independent administrator-managed and IdP-managed grants:
OIDC reconciliation must replace only the latter; explicit administrator grants
must survive until an administrator removes them.

Existing assignment rows do not record reliable ownership. Null actor
attribution can represent OIDC, bootstrap/local writes, or a removed assigning
administrator. The Users page's role/source labels and `all` environment summary
are not evidence of individual grant ownership.

This slice does not backfill ownership or alter existing account grants.
Unclassified legacy assignments remain an explicit OIDC PAT rollout blocker.
A source-aware migration/cutover and recovery policy must be established before
OIDC-backed PATs are admitted. Independent grants are the agreed future policy;
preserving current rows is not evidence that fail-closed reconciliation or
entitlement freshness exists yet.

## Remaining release gates

M0 remains incomplete. The following work is still required before PAT/MCP
release:

1. Source-aware OIDC assignment reconciliation, successful authoritative sync
   time, maximum entitlement age, failure/rollback behavior and legacy cutover.
2. Token-row validation for every admitted request and transaction-aware checks
   for any future mutation, with deterministic revocation ordering.
3. Binding the credential to the authenticated current owner; role and
   environment claims must never come from client-supplied arguments.
4. Exact per-tool capability checks, active/eligible resource policy, bounded
   scoped queries, safe POA&M projections and audit/redaction.
5. Request-boundary and concurrent revocation/entitlement acceptance tests.

No token-grant type, unit test, or build result substitutes for these gates.

## Verification scope

Unit tests cover environment intersection, Admin bounds, empty grants, exact
scope conjunction, audience rejection and expiry equality. The SQLx regression
`restricted_credential_refresh_preserves_grant_and_current_owner_intersection`
uses real migrated PostgreSQL and exercises service refresh, scope denial,
promotion, demotion, membership removal, account disable, shared-plan denial
and restricted mutation denial independent of owner Admin status.

The existing state-machine database regression runner already runs the entire
`poam_workflows` integration target. Adding this test does not create another
top-level check. Execution results, failures and unrun integration-owner gates
are recorded in TASK-526.2. Builds after the owner's build-farm instruction use
`--max-jobs 0`; no local Cargo compilation is authorized.
