---
id: TASK-534.4
title: M2 read-only MCP server
status: In Progress
assignee: []
created_date: '2026-11-20'
labels: [security, agent-access, mcp]
parent_task_id: TASK-534
dependencies:
  - TASK-534.3
references:
  - TASK-534
  - TASK-534.3
  - docs/knowledge/security/agent-access-tokens-mcp-remediation-context.md
priority: high
---

## Description
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

TASK-534.3 issues `cfat_v1_` access tokens, but nothing accepts them. An agent
cannot read CVE or POA&M work from Crystal Forge.

## Goal

cf-server exposes an MCP endpoint at `/mcp`, off by default. A coding agent
configured with a static `Authorization: Bearer cfat_v1_...` header can call four
read-only tools. Every result is bounded by the token's grant intersected with
the owner's current role and environments. Admins see MCP status in Server
Management; the token reveal view shows the real endpoint URL.

## Non-goals

- Mutating tools (`request_cve_scan`, `update_poam_progress`, `verify_poam`,
  `close_poam`) and their scopes (M4).
- A partial scoped POA&M projection (`visibility: "scoped"`). `get_poam_detail`
  returns a POA&M only when every subject is authorized, otherwise 404.
- Source and module attribution (TASK-271). Reported as unknown.
- MCP OAuth authorization, SSE streams, server-initiated messages, sessions,
  resources, prompts, sampling.
- Admin token tab, Access Inspector, Connect agent shortcut (TASK-537).

## Decisions

- Hand-written JSON-RPC 2.0 over Streamable HTTP, no new dependency. Stateless:
  POST only, one JSON-RPC message per request, `application/json` responses, no
  `Mcp-Session-Id`. GET and DELETE return 405. Notifications return 202.
- Protocol versions accepted: `2025-11-25` (preferred), `2025-06-18`,
  `2025-03-26`. `initialize` echoes the client version when supported, else
  offers `2025-11-25`. Later requests with an unsupported `MCP-Protocol-Version`
  header get 400.
- Authentication on every request: bearer `cfat_` token through
  `services::access_tokens::validate_presented_token`. No cookies, no query
  tokens. Failure is 401 with `WWW-Authenticate: Bearer` and a JSON-RPC error.
- Origin: if an `Origin` header is present it must equal the origin of the
  configured public URL, else 403. Requests without `Origin` (CLI agents) pass.
- Limits: 1 MiB body, 30 s per request, 4 concurrent requests per token (429).
- `last_used_at` written at most once per 5 minutes per token.
- Configuration: `mcp_enable` (default false) and `mcp_public_url` (optional;
  https, or http on loopback). NixOS `services.crystal-forge.server.mcp.{enable,publicUrl}`.
  With MCP disabled, `/mcp` returns 404 and the access-token policy reports
  `mcp.enabled = false`.

## Tools

Each tool computes effective environments as
`CredentialIdentity::Restricted(grant).reading_environments(owner_is_admin, owner_environments, required_scopes, now)`
restricted to active environments. Missing scopes give a tool error, not data.
A hidden or nonexistent system or POA&M gives the same `not_found` error.

| Tool | Scopes | Source |
| --- | --- | --- |
| `list_remediation_work` | `cves:read` (+ `poams:read` for `include_poams`) | New keyset query over `view_current_exact_cve_occurrences`, filtered by effective environments in SQL; POA&M references through `cve_relationships_for_rows` per system on the page. |
| `get_system_security_context` | `cves:read` | `find_system_access_row` environment check, then `fetch_system_cve_inventory_page`. |
| `get_remediation_context` | `cves:read`, `provenance:read` | Same system check, inventory rows for the CVE and package, plus one query for store path, derivation and commit; attribution `unknown`. |
| `get_poam_detail` | `poams:read` | Full detail only when every POA&M subject is in the effective set; otherwise `not_found`. |

`tools/list` returns only tools whose scopes the token holds. `tools/call`
reauthorizes every call. Collections default to 25 items, maximum 100, with
opaque cursors. Text from CVE data is untrusted: control characters removed and
long descriptions truncated.

## Verification plan

Owner instruction (2026-11-20): implement quickly, keep local test cycles short,
let MR CI run the suites; the owner evaluates on the dev server. One farm compile
of the cf-server library and test targets before push.

## Impact, risk and dependencies

- Impact: new `mcp` module and route, config and NixOS options, new CVE work-item
  query, POA&M restricted detail path, Admin MCP status endpoint and card,
  access-token policy MCP fields, docs.
- Risk: high. A bearer endpoint exposes CVE and POA&M data to agents.
- Dependencies: stacked on TASK-534.3 (!346), which is stacked on !345.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 With MCP enabled, an MCP client using a valid token completes `initialize`, `tools/list` and `tools/call` over POST `/mcp`; MCP disabled gives 404.
- [ ] #2 Every request without a valid, unrevoked, unexpired token from an active owner is rejected with 401 before any tool runs; cookies and query tokens never authenticate.
- [ ] #3 `tools/list` shows only tools allowed by the token's scopes; each call reauthorizes against the current owner state.
- [ ] #4 Tool results never include systems, findings or POA&Ms outside the effective environments; hidden and nonexistent resources are indistinguishable.
- [ ] #5 Admin Server tab shows MCP status and URL; the token reveal shows the configured endpoint.
- [ ] #6 Docs describe client configuration, tools, limits and what is unsupported.
<!-- AC:END -->

## Implementation Notes
<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode on vm-agent in /home/mcamp/code/crystal-forge/TASK-534.4-read-only-mcp-server

Selected by the owner on 2026-11-20. Branch `TASK-534.4-read-only-mcp-server` starts at `aca87741` (TASK-534.3 head).
<!-- SECTION:NOTES:END -->

## Review !347 response and verification at `0e0349cf`

Review findings fixed: P2-1 `get_remediation_context` now follows the inventory cursor until the exact CVE and package match or the inventory ends (regression `remediation_context_finds_a_package_beyond_the_first_inventory_page`, 520 packages for one CVE). P2-2 one 30 s deadline now covers token validation, the `last_used_at` write, the body read and the tool call; an expiry before the tool call returns 503 with -32603 (regression `slow_request_body_is_cut_off_by_the_request_deadline`).

Farm PostgreSQL run (output `nkjzw7jp7wf9fjjjailk2z0kg6y6m4ps-cf-mcp-regressions-0.3.0`): `access_tokens` 43 passed, `mcp_tools` 8 passed, `mcp_transport` 10 passed; 0 failed, 0 ignored.

Independent client check on the dev server (2026-11-20, OpenCode MCP client, read-only, owner's token): `initialize`, `tools/list` and `tools/call` succeeded for all four tools. Observed: paged work list with a working cursor; a cursor reused with another filter rejected; an environment outside the token rejected; system context for a managed system with exact current scan, running target and severity filter; remediation context with store path, derivation and commit, lowercase CVE id normalized, attribution unknown; unknown system, unknown package and unknown POA&M all return the same `not_found`. Not exercised on the dev server: a real POA&M (none linked to the visible findings) and live role, membership or revocation changes while a client is connected (covered only by the database tests).
