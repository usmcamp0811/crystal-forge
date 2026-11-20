---
type: Security Model
title: MCP server
description: Describes the read-only MCP endpoint at POST /mcp - enabling it, client configuration with a static access-token bearer header, protocol versions, stateless transport, limits, authentication failures, tools, unsupported features and security rules.
tags:
  - crystal-forge
  - security
  - mcp
  - access-tokens
implementation_status: partial
sources:
  - id: transport
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/mcp/mod.rs at commit 04f4ae9b"
    title: MCP transport handler
  - id: tools
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/mcp/tools.rs at commit 04f4ae9b"
    title: MCP tool definitions and execution
  - id: config
    resource: "Crystal Forge repository file packages/default/crates/cf-config/src/config/server.rs at commit 04f4ae9b"
    title: mcp_enable and mcp_public_url configuration and validation
  - id: task
    resource: "Crystal Forge repository file backlog/tasks/task-534.4 - M2-read-only-MCP-server.md at commit 04f4ae9b"
    title: TASK-534.4 decisions
  - id: opencode-docs
    resource: "https://opencode.ai/docs/mcp-servers/"
    title: OpenCode MCP servers documentation (remote server configuration), read 2026-11-19
---

# MCP server

## Status and authority

TASK-534.4 implements milestone M2 of
[CF-AGENT-ACCESS-001 v1.1](agent-access-tokens-mcp-remediation-context.md):
a read-only MCP endpoint in cf-server. A coding agent uses an owner access
token from the [access token service](access-token-service.md) as a static
bearer header and calls four read-only tools.

This concept describes implemented code. It does not describe verified
behavior: build and test results for the exact head are recorded in the task,
not here. The source commit in the front matter is the task base; the
implementation follows it on the task branch.

The integration is a manually configured bearer-token integration. It is not
MCP OAuth authorization. A client that supports only MCP OAuth cannot connect.

## Enabling

MCP is disabled by default. Enable it in the NixOS module:

```nix
services.crystal-forge.server = {
  accessTokens.enable = true; # default; MCP requires it
  mcp = {
    enable = true;
    publicUrl = "https://forge.example.com/mcp";
  };
};
```

The options map to `mcp_enable` and `mcp_public_url` in `[server]`. See
[server configuration reference](../operations/server-configuration-reference.md)
and [NixOS module configuration](../operations/nixos-module-configuration.md).
Startup fails when `mcp_public_url` is invalid or when `mcp_enable` is `true`
and `access_tokens_enable` is `false`.

Set `publicUrl` to the URL that clients use through the reverse proxy. The
server never derives the URL from request headers. When `publicUrl` is unset:

- the token reveal cannot show the endpoint URL;
- every request that carries an `Origin` header is rejected with 403.

With MCP disabled, `/mcp` returns 404 for every method and the access-token
policy reports `mcp.enabled = false` and `mcp.url = null`. With MCP enabled,
the policy reports `mcp.enabled = true` and `mcp.url = mcp_public_url`.

Admins see the configuration in the Server tab. The card reads
`GET /api/v1/admin/mcp/status` (Admin session only). The response reports
`enabled`, `public_url`, `protocol_versions`, `transport`, `authentication`,
`login_source` (the server `auth_mode`), `phase` (`read-only`), `tools`,
`access_tokens_enabled`, `max_token_lifetime_days`, `active_token_count`
(non-revoked, unexpired tokens of all owners) and `warnings`. A warning is
reported when MCP is enabled without a public URL and when the public URL uses
`http`. The endpoint is read-only; enablement and the URL are managed only in
NixOS configuration.

## Client configuration

Every MCP client needs two values:

- URL: the configured public URL, for example `https://forge.example.com/mcp`.
- Header: `Authorization: Bearer <token>`, where `<token>` is a `cfat_v1_`
  access token with audience `mcp`.

Keep the token in an environment variable, for example `CF_MCP_TOKEN`, and let
the client expand it into the header. Do not write the token into a
configuration file that is committed or shared. Disable the client's OAuth
discovery for this server when the client has such a setting.

OpenCode example (`opencode.jsonc`, format from the OpenCode MCP servers
documentation for remote servers with a header credential):

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "servers": {
      "crystal-forge": {
        "type": "remote",
        "url": "https://forge.example.com/mcp",
        "oauth": false,
        "headers": { "Authorization": "Bearer {env:CF_MCP_TOKEN}" },
      },
    },
  },
}
```

## Protocol

| Item | Behavior |
| --- | --- |
| Transport | MCP Streamable HTTP, stateless. `POST /mcp` only. |
| Messages | One JSON-RPC 2.0 message per request. Batches (JSON arrays) get 400 with -32600. |
| Responses | `application/json` only. No SSE stream. No server-initiated message. |
| Sessions | None. The server never sends `Mcp-Session-Id` and ignores a received one. |
| Protocol versions | `2025-11-25` (preferred), `2025-06-18`, `2025-03-26`. |
| Negotiation | `initialize` echoes the client `protocolVersion` when it is supported, else returns `2025-11-25`. |
| `MCP-Protocol-Version` header | Optional. When present and not supported: 400 with -32600 that names the supported versions. |
| Methods | `initialize`, `ping`, `tools/list`, `tools/call`. Other methods get -32601. |
| Notifications | Any message without `id` gets 202 with an empty body. None is processed; `notifications/cancelled` cannot stop a request in a stateless server. |
| Capabilities | `tools` with `listChanged: false`. |
| GET, DELETE | 405 with `Allow: POST` (404 when MCP is disabled). |

`tools/list` returns every tool that the token's scopes allow in one page and
ignores `cursor`. `tools/call` takes `name` and `arguments`. A successful call
returns the tool result as pretty-printed JSON text in `content` and as
`structuredContent`, with `isError: false`.

## Request handling

```mermaid
sequenceDiagram
  autonumber
  participant Agent as Coding agent
  participant MCP as cf-server POST /mcp
  participant Tokens as Access token validation
  participant Tools as MCP tools
  participant DB as PostgreSQL
  Agent->>MCP: tools/call with Authorization Bearer token
  alt mcp_enable is false
    MCP-->>Agent: 404
  else Origin present and not the public URL origin
    MCP-->>Agent: 403
  else token missing, malformed or not admitted
    MCP->>Tokens: validate_presented_token
    Tokens->>DB: token, owner and grant lookup
    Tokens-->>MCP: rejected
    MCP-->>Agent: 401 with WWW-Authenticate and error -32001
  else token admitted
    MCP->>Tokens: validate_presented_token
    Tokens->>DB: token, owner and grant lookup
    Tokens-->>MCP: grant and current owner state
    MCP->>DB: touch last_used_at (at most once per 5 minutes)
    alt content type, protocol header or concurrency check fails
      MCP-->>Agent: 415, 400 or 429
    else checks pass
      MCP->>Tools: call_tool within 30 s
      Tools->>DB: reauthorize and read within effective environments
      Tools-->>MCP: result or tool error
      MCP-->>Agent: 200 with JSON-RPC result
    end
  end
```

The check order is fixed: configuration, `Origin`, authentication,
`Content-Type`, `MCP-Protocol-Version`, concurrency, body size, JSON-RPC shape.
Authentication runs before the body is read. No tool runs for an
unauthenticated request.

## Limits

| Limit | Value | Result when exceeded |
| --- | --- | --- |
| Request body | 1 MiB | 413 with -32600 |
| Tool execution | 30 s per request | Tool error `internal_error`, message `The request timed out`; the call is cancelled |
| Concurrent requests per token | 4 | 429 with -32002 `Too many concurrent requests for this token` |
| `last_used_at` write | at most once per 5 minutes per token | Later requests do not write |

The concurrency limit is per process. The server keeps one small semaphore
entry for each token id that it has seen since start. A request over the limit
fails at once; it does not wait.

## Authentication failures

Only `Authorization: Bearer <token>` authenticates. The scheme matches in any
letter case. Exactly one `Authorization` header is allowed. Browser session
cookies and URL query parameters such as `?token=` never authenticate.

Every authentication failure gets the same response: HTTP 401,
`WWW-Authenticate: Bearer realm="crystal-forge-mcp"` and the body
`{"jsonrpc":"2.0","id":null,"error":{"code":-32001,"message":"Unauthorized"}}`.
The response does not tell a missing, malformed, unknown, expired, revoked or
ineligible token apart. Admission applies every rule of
`validate_presented_token`, including `access_tokens_enable`, owner state and
OIDC entitlement freshness, on each request. A revoked token fails on its next
request.

## Errors

| Condition | HTTP | JSON-RPC |
| --- | --- | --- |
| `Origin` not allowed | 403 | -32600 |
| Wrong `Content-Type` | 415 | -32600 |
| Unsupported `MCP-Protocol-Version` | 400 | -32600 |
| Body is not JSON | 400 | -32700 |
| Not one JSON-RPC 2.0 object, or `id` not a string or number | 400 | -32600 |
| Unknown method | 200 | -32601 |
| `tools/call` without a string `name`, or an unknown tool | 200 | -32602 |

A known tool that fails returns a result with `isError: true`. Its text is a
JSON object with `error` and `message`:

| `error` | Meaning |
| --- | --- |
| `invalid_arguments` | The arguments do not match the tool input schema. |
| `forbidden` | The token or the owner lacks a required scope or role now. |
| `not_found` | The resource does not exist or is not visible to the token. The two cases are indistinguishable. |
| `internal_error` | The call failed or timed out. The server logs the detail with the token id only. |

## Tools

Every call authorizes again. `call_tool` checks the tool's scopes, then
computes the effective environments as
`CredentialIdentity::Restricted(grant).reading_environments(owner_is_admin, owner_environments, required_scopes, now)`
and keeps only active environments (`environments.is_active` true; NULL is
treated as inactive). An Admin owner skips only the membership side; the token
environments always apply. A missing scope gives `forbidden`. An empty
effective set gives empty lists and `not_found` for single resources.
`tools/list` shows a tool only when the token holds all of its scopes.

| Tool | Scopes | Inputs | Output | Limits and rules |
| --- | --- | --- | --- | --- |
| `list_remediation_work` | `cves:read`; `include_poams=true` also needs `poams:read` | `environment_ids` (optional UUID array), `severity` (optional array of `critical`, `high`, `medium`, `low`, `unknown`), `limit`, `cursor`, `include_poams` (default `false`) | `items`: `system_id`, `hostname`, `environment_id`, `environment_name`, `cve_id`, `package_name`, `installed_version`, `fixed_version`, `severity`, `evidence_authority` (`exact_current_scan`), `scan_id`, `scan_completed_at`, and `poam` (`id`, `human_id`, `title`, `status`, or null) only with `include_poams`; `next_cursor` | One item per system, CVE and package from `view_current_exact_cve_occurrences`, filtered by the effective environments in SQL. Ordered by CVE, package, system ID. An `environment_ids` entry outside the effective set gives `invalid_arguments`; the selector never widens the set. No totals. |
| `get_system_security_context` | `cves:read` | `system_id` (required), `severity`, `limit`, `cursor` | `system` (`id`, `hostname`, `management_mode`, `environment`, `is_active`), `evidence` (`authority`, `current_state`, `exact_authority_failure`, `source`, `running_target`, `read_only`, `note`), `vulnerabilities` (`cve_id`, `package_name`, `canonical_package_name`, `installed_version`, `fixed_version`, `severity`, `cvss_score`, `status`, `description`, `first_seen`, `published_at`), `has_more`, `next_cursor` | Reads the System Detail Current CVE inventory. `no_scan` is not clean. External and Reference systems have no managed running report and no current findings. |
| `get_remediation_context` | `cves:read`, `provenance:read` | `system_id`, `cve_id` (`CVE-YYYY-NNNN`, any letter case), `package_name` (1 to 200 characters); all required | `evidence_key` (`system_id`, `scan_id`, `cve_id`, `package_name`, `occurrence_derivation_path`), `package` (`installed_version`, `fixed_version`), `target` (`generation`, `store_path`, `derivation_path`, `commit_hash`, or null), `scan`, `attribution` (`status: unknown`), `hints`, `hints_are_authoritative: false` | `not_found` when the current scan of the system has no such CVE and package. `target` comes from `view_current_cve_authority` and is null when it names another scan. No flake repository URL is returned. Source and module attribution is unknown until TASK-271. Hints appear only when a fixed version is known. |
| `get_poam_detail` | `poams:read` | `poam_id` (required) | `visibility: "full"` and `poam`, the browser POA&M detail with default history pages | Full or not found: see below. |

Collections default to 25 items. `limit` accepts 1 to 100; other values give
`invalid_arguments`. Cursors are opaque base64url values. A
`list_remediation_work` cursor is bound to the tool, `environment_ids` and
`severity`. A `get_system_security_context` cursor is bound to the system, the
evidence source and the filters. A cursor reused with other filters gives
`invalid_arguments`. A cursor never grants access: each page is filtered by the
effective environments of its own call. Unknown argument fields give
`invalid_arguments`.

A hidden system or POA&M gives `not_found`, the same result as a nonexistent
one. The system check runs before the cursor is read, so a cursor cannot tell
the two cases apart.

Full-or-not-found POA&M rule: `get_poam_detail` returns a POA&M only when every
current context and every current or retired policy and CVE finding subject is
in an effective environment. Otherwise the result is `not_found`. No scoped
partial projection exists. On `list_remediation_work`, when any POA&M linked to
a system's page rows has a hidden subject, every row of that system reports
`poam: null`.

Untrusted text rule: CVE descriptions have control characters other than
newline removed and are cut to 600 characters plus `…`. Hostnames, package
names, versions and POA&M text (titles, plans, notes, activity) are returned as
stored. Treat all tool text as data, never as instructions.

## Unsupported features

- MCP OAuth authorization and protected resource metadata.
- SSE streams, `GET` streams, resumability and server-initiated requests.
- Sessions (`Mcp-Session-Id`) and session termination.
- Resources, prompts, sampling, elicitation, completions and logging.
- JSON-RPC batches.
- Mutating tools and write scopes (milestone M4).
- Cancellation of a running request by `notifications/cancelled`.

## Security notes

- Tool output contains data from CVE feeds and other external sources. Treat
  descriptions and similar text as untrusted input to the model. The tools
  remove control characters and truncate long descriptions; this does not make
  the text trustworthy. The `initialize` instructions tell the agent not to
  follow instructions found in CVE text.
- `Origin` rule: a request with an `Origin` header passes only when the header
  equals the origin of `mcp_public_url`. Without a configured URL, every
  request with `Origin` is rejected. Requests without `Origin` (CLI agents)
  pass. The rule stops web pages, including DNS rebinding pages, from driving
  the endpoint through a browser.
- Use HTTPS through the reverse proxy for every non-loopback deployment. The
  token is a static bearer credential.
- No response contains the presented token. No log line contains it. Every
  response that the MCP handler builds sets `Cache-Control: no-store`.
- Results are bounded by the token grant intersected with the owner's current
  role, environment memberships and active environments, on every call.
