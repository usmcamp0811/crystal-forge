---
id: TASK-534.3
title: M1 owner access-token service and Profile UI
status: In Progress
assignee: []
created_date: '2026-11-19'
labels: [security, authorization, agent-access, ui]
parent_task_id: TASK-534
dependencies:
  - TASK-534.2
references:
  - TASK-534
  - TASK-534.2
  - docs/knowledge/security/agent-access-tokens-mcp-remediation-context.md
priority: high
---

## Description
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

M0 (TASK-534.2, MR !345) bounds what a restricted credential may do, but no
credential can be created. A signed-in user has no Access Tokens section in
Profile and no API to issue, inspect, rotate or revoke a `cfat_v1_` token.

## Goal

Any signed-in user (Viewer, Operator, Admin; local or OIDC) can create, list,
inspect, rotate and revoke their own access token from Profile, without Admin
approval. The secret is shown once and only its SHA-256 hash is stored. The
server can parse and validate a presented token against current owner state, so
M2 (MCP) only has to call it.

## Non-goals

- MCP transport, tools, `/mcp` route, MCP status card, MCP server option.
  The policy endpoint reports `mcp.enabled = false` until M2.
- Admin Access Tokens tab, admin list/revoke/revoke-all endpoints, Access Inspector
  (spec 10.5 and 10.7). Admin tab: TASK-537; Inspector: M2.
- Connect agent shortcut on Fleet CVEs and POA&M views (spec 10.1.1). TASK-537.
- Write or operational scopes. Only `cves:read`, `poams:read`, `provenance:read`
  are selectable; `scans:request`, `poams:write`, `poams:verify`, `poams:close`
  are listed as unavailable (`Not supported yet`).
- Service principals, CLI audience, OAuth.
- Editing a token grant after issuance. Grants are immutable.

## Architectural constraints

- The server owns authorization. Management endpoints accept only a browser session
  with CSRF. A `cfat_` bearer credential is never accepted by them.
- Token endpoints use no Admin guard. An Operator has full self-service.
- Hash only: store SHA-256 of the complete canonical token. Never log, audit or
  return the secret except in the single create/rotate response, which carries
  `Cache-Control: no-store`.
- Reuse the M0 `RestrictedGrant`/`CredentialIdentity` boundary. Do not invent a
  second authorization path.
- Forward migration `0336` only. `0335` is frozen.
- Create and rotate are serialized per owner (advisory lock on the owner row) so
  count, name uniqueness and revoke-then-issue are atomic.
- Browser secret handling: the secret lives only in component state of the reveal
  view. It is not stored in browser storage, URL, global state or logs.

## API contract (all under `/api/v1`, JSON, `Cache-Control: no-store`)

Errors use `{"error": code, "message": text, "details": null|object}` with codes
`validation_failed` (400, `details.fields` maps field to text), `unauthorized`
(401), `forbidden` (403), `access_tokens_disabled` (403), `resource_not_found`
(404, also for another user's token), `conflict` (409), `rate_limited` (429).

| Method | Path | Notes |
| --- | --- | --- |
| GET | `/user/access-token-policy` | enabled, mcp, lifetime options, selectable environments and scopes, `create_blocked` |
| GET | `/user/access-tokens` | own tokens; query `status`, `q`, `limit` (default 25, max 100), `offset` |
| POST | `/user/access-tokens` | session + CSRF; 201 with metadata and `token` once |
| GET | `/user/access-tokens/:id` | own token; metadata plus `effective` and `recent_events` |
| DELETE | `/user/access-tokens/:id` | session + CSRF; 200 with metadata; idempotent |
| POST | `/user/access-tokens/:id/rotate` | session + CSRF; 201 like create, plus `replaced_token_id` |

`AccessTokenDto`: `id`, `name`, `audience` (`"mcp"`), `display_hint`
(`cfat_v1_8d12…`), `scopes` (sorted strings), `environments`
(`[{id, name, is_active}]`), `created_at`, `expires_at`, `revoked_at`,
`last_used_at` (RFC 3339 or null), `status` (`active`, `expiring_soon` within 7
days, `expired`, `revoked`, `owner_suspended`). No secret, no hash.

Create body: `{name, audience: "mcp", expires_in_days, environment_ids, scopes}`.
Create response: `AccessTokenDto` fields plus `token`. Detail adds
`effective: {environments: [{id, name, available}], scopes: [{scope, permitted}]}`
(granted versus current effective) and `recent_events: [{action, created_at}]`
(at most 10).

Policy response: `{enabled, mcp: {enabled, url}, audience, default_lifetime_days,
max_lifetime_days, lifetime_options_days, max_active_tokens,
environments: [{id, name}], scopes: [{scope, label, available, default_selected,
unavailable_reason}], create_blocked: null | {code, message}}`.

Limits: name 1–100 chars, unique per owner among non-revoked tokens
(case-insensitive); at most 20 non-revoked unexpired tokens per owner; at most 50
environments per token; at most 10 create or rotate operations per owner per hour
(429, counted from `created_at`); lifetime 1..=`max_lifetime_days` (default 30,
max 90). Create never silently narrows a request: unknown, inactive, duplicate or
unassigned environment ids and unavailable or duplicate scopes are rejected.

Rotate: only for an active (not revoked, not expired) token. Revalidates the same
grant against current policy and current owner permissions; a grant that is no
longer permitted gives 409 and changes nothing. The replacement keeps name,
audience, scopes and environments, expires after the original lifetime clamped to
the current maximum, and the old token is revoked in the same transaction.

OIDC owners: create and rotate require fresh entitlements and resolved ownership
(M0 `require_fresh_entitlements`); otherwise `create_blocked` is set and the
request is rejected with 409. Token validation applies the same rule.

Audit events (`admin_audit_events`, written in the same transaction as the
change): `access_token_created`, `access_token_rotated`, `access_token_revoked`;
target is the public token id; metadata holds name, scopes, environment ids and
expiry, never the secret or hash. They are visible in the Admin Audit Log.

## Configuration

`ServerConfig`: `access_tokens_enable` (default true; tokens are inert until M2
enables MCP), `access_tokens_default_lifetime_days` (30),
`access_tokens_max_lifetime_days` (90). NixOS options
`services.crystal-forge.server.accessTokens.{enable,defaultLifetimeDays,maxLifetimeDays}`.
Startup validation: `1 <= default <= max <= 90`.

## Verification plan

- Farm: PostgreSQL HTTP regressions (`access_tokens` test target) for lifecycle,
  ownership isolation, role/environment bounds, CSRF, hash-only storage, expiry,
  revoke/rotate atomicity, rate limit, OIDC freshness block, validation function.
- Farm: embedded app build with Web UI unit tests; `cf-config` tests.
- Farm: `integration-access` scenario cases for local Operator create, reveal,
  rotate, revoke through the real browser, plus an Operator who cannot reach `/admin`.
- `nix build .#okf-knowledge -L`.
- Manual: preview at the task URL, Operator and Admin users.

## Impact, risk and dependencies

- Impact: migration, server auth/service/handlers, config and NixOS module, Audit
  Log enum, Profile UI, Access scenario, security docs.
- Risk: high. A bug here issues credentials. Mitigated by hash-only storage,
  per-owner serialization, M0 grant boundary and no consumer yet (M2).
- Dependencies: TASK-534.2 (!345). This branch is stacked on
  `TASK-529.2-agent-authorization`; its MR targets that branch until !345 merges,
  then retargets `dev`. Migration `0336` and an upstream scan-cache-recovery
  migration range (0335 to 0338 in another worktree) may need renumbering at
  rebase time.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A Viewer, Operator and Admin (local and OIDC) can create a token for explicit environments within their current permissions without Admin approval; the secret is returned once with `no-store` and only its SHA-256 hash is stored.
- [ ] #2 Create validates name, audience, lifetime, scopes and environments, rejects (never narrows) unauthorized grants, and enforces name, count and rate limits with deterministic transactional ordering.
- [ ] #3 List, detail, revoke and rotate are owner-only (404 for another user's token), require session plus CSRF, and a bearer token is rejected by every management endpoint.
- [ ] #4 Revoke is irreversible and immediate; rotate atomically replaces the token, revalidates the grant and revokes the old token in one transaction.
- [ ] #5 Server-side token validation rejects malformed, unknown, wrong-hash, expired, revoked and disabled-owner tokens and reports the immutable grant bounded by the owner's current role and environments; OIDC owners are blocked on stale or unresolved entitlements.
- [ ] #6 Profile shows an Access Tokens card for every signed-in user with list, create modal, one-time reveal, details, rotate and revoke, with loading, empty, error, disabled and pending states; the secret is not persisted in the browser.
- [ ] #7 Audit events appear in the Admin Audit Log; configuration and NixOS options exist with validation.
- [ ] #8 Farm results for the database regressions, embedded build, Access scenario and OKF validation are recorded for the exact head; documentation matches the implemented behavior.
<!-- AC:END -->

## Implementation Notes
<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode on vm-agent in /home/mcamp/code/crystal-forge/TASK-534.3-access-token-lifecycle

Selected by the user on 2026-11-19: stack on the !345 branch and deliver the full owner workflow first. Branch `TASK-534.3-access-token-lifecycle` starts at `a2a61991`.
<!-- SECTION:NOTES:END -->

## Verification at head `db85d668`

All results are for source revision `db85d6682445eb6e591f076a97348ea93e77bc45`. Acceptance items stay unchecked until the maintainer reviews the MR.

- PostgreSQL/HTTP regressions PASSED on the farm: `access_tokens` 41 passed, `admin_source_grants` 28, `oidc_entitlements` 24; 0 failed, 0 ignored (harness `/tmp/opencode/task-534-3-regression.nix`, 0336 applied by `sqlx::test`).
- Embedded application build PASSED (`default.cf-server-drv`): server library 1607 passed, 0 failed, 856 ignored. Web UI derivation: 938 passed, 0 failed, 1 ignored.
- `integration-access` scenario `lifecycle-local-bootstrap` PASSED: 56 of 56 cases (47 existing plus 9 token cases), 643 s. The evidence `meta.json` records `db85d668`. Timeout raised from 900 to 1500 s: a run at the earlier head hit 900 s during an existing fragment on a loaded farm (cases ran about 1.5 times slower) and so did not reach the token cases; that run is not a result.
- OKF validation PASSED (`nix build .#okf-knowledge`).
- Not run for this head: the core package native post-checks and the `cf-config` unit tests as a separate target (the config tests run inside the core package build, which was not rebuilt for this head). Exact-head GitLab CI is unverified.

Defects found and fixed by review of the preview and gate evidence: the Expiration select displayed "7 days" while state and request used 30 (a `value` on a select applies before its options exist); the create and reveal dialog headings were squeezed beside their descriptions.

An independent security review found no bypass of owner scoping, CSRF or hash-only storage. Fixed: kill switch now stops admission (`validate_presented_token` takes settings), list and detail work while disabled so an owner can still revoke, rotate rejects an empty environment set and admission fails closed on it, revoke takes the owner row lock first (deadlock with user delete), unique-index violation maps to 409, name search rejects control characters, bearer detection tolerates whitespace and case, validation failure logging no longer uses `Debug`, `recent_events` avoids a full scan. Deferred and documented as known limits: TASK-536 (audit window flooding, OIDC eligibility under the owner lock, session validity in the transaction, smaller items). Admin tab and Connect agent shortcut: TASK-537.

Preview (isolated): http://127.0.0.1:8529 serves this build; `preview-operator` (Operator, `dev` only), `preview-viewer`, `preview-admin`; see `/tmp/opencode/task529-preview/HANDOFF.md` round 5.
