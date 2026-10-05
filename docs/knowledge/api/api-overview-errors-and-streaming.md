---
type: API
title: "Backend API overview, error codes, and WebSocket streaming"
description: "Describes the REST base URL, authentication, response and error shapes, common error codes, the evaluation-log WebSocket stream, and the per-resource endpoint summary table; open it first when working with the Crystal Forge HTTP API."
tags:
  - crystal-forge
  - api
  - rest
  - websocket
  - errors
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T21:00:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/specs/02-backend-api.md at commit 3b23d36f"
    title: "Backend API Specification"
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/bin/server.rs at commit 3b23d36f"
    title: "server.rs implementation"
  - id: code-2
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/api/commits.rs at commit 3b23d36f"
    title: "commits.rs implementation"
  - id: code-3
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/api/models.rs at commit 3b23d36f"
    title: "models.rs implementation"
  - id: code-4
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/api/rbac.rs at commit 3b23d36f"
    title: "rbac.rs implementation"
---

# Backend API Specification

This document describes Crystal Forge's HTTP API. It is written for developers who need to know how the server responds, which endpoints exist, and how to add new ones.

**Assumption:** You understand HTTP, REST APIs, and basic database concepts.

## API Overview

The server exposes a **REST API** that the Dioxus Web UI uses. Agents and builders use separate signed-request routes on the same server.

**Base URL:** `http://<server-host>:<port>/api/v1/`

The port is deployment configuration. The NixOS module option `services.crystal-forge.server.port` defaults to `3000`. The server binds `0.0.0.0` and speaks plain HTTP. TLS termination is a deployment concern, normally a reverse proxy.

### Route families

| Prefix | Caller | Authentication |
| --- | --- | --- |
| `/api/v1/...` | Web UI and API clients | Browser session cookie |
| `/api/auth/...` | Web UI | Login, logout, and session routes. See [API authentication](../security/api-authentication-and-authorization.md). |
| `/api/v1/builders/:id/...` | Builders | Signed builder request |
| `/agent/...`, `/system_state` | Agents | Signed agent request |
| `/webhook`, `/status` | Git forges, probes | See [Core components](../components/core-components.md) |

### Request format

- **Headers:** `Content-Type: application/json`.
- **Body:** JSON for `POST`, `PUT`, and `PATCH` requests.
- **Authentication:** The `__Host-cf-session` cookie. See [Session cookies and CSRF](../security/session-cookies-and-csrf.md).
- **CSRF:** The Web UI sends the `x-csrf-token` header on requests that change state. Its value must equal the `__Host-cf-csrf` cookie. A handler enforces this check only when it calls `require_csrf`, and a mismatch returns HTTP 403 with `csrf_validation_failed`. No global layer applies the check, and the role guard extractors do not apply it. Do not assume that a mutating route rejects a request without the header.

### Response format

**Success.** A handler returns its response DTO as the JSON body. There is no `data` wrapper. For example, the evaluation queue returns an `EvalQueueSummary` object (see [Builders, queues, environments, dashboard, and admin APIs](builders-queues-environments-dashboard-admin-api.md)).

**Pagination.** There is no shared pagination envelope. Each list endpoint defines its own query parameters and count fields in `api/models.rs`. For example, `GET /api/v1/admin/audit-events` takes `page` and `per_page`, and the evaluation queue returns `filtered_total` with `items[]`.

**Error.** Errors use the flat `ApiError` DTO:

```json
{
  "error": "not_found",
  "message": "System not found",
  "details": null
}
```

`error` is a lowercase machine code. `message` is human-readable text. `details` is optional, and the server omits it when empty.

## Common Error Codes

| HTTP | `error` | Meaning | When used |
| --- | --- | --- | --- |
| 400 | `validation_error` | Invalid input | Bad request data |
| 401 | `unauthorized` | No valid session | Handlers that use the `RequireAuth`, `RequireOperator`, or `RequireAdmin` extractors, and signed agent or builder verification |
| 403 | `forbidden` | Not allowed | Insufficient role. Handlers that use the `authenticated_user_roles` helpers also answer 403 for a missing, expired, or invalidated session. |
| 403 | `csrf_validation_failed` | CSRF check failed | State-changing browser request without a matching token |
| 404 | `not_found` | Resource is missing or hidden | The ID is wrong, or environment scope hides the resource |
| 409 | `conflict` | State conflict | Duplicate create, or a request that conflicts with current state |
| 500 | `internal_error` | Server failure | Unexpected persistence or service error |

Some compliance handlers use upper-case domain codes such as `POLICY_INTERCHANGE_INVALID` and `ASSIGNMENT_TARGET_NOT_FOUND`.

**Hidden resources.** Handlers return `not_found`, not `forbidden`, for a resource that environment scope hides from the caller. A caller therefore cannot learn that the resource exists.

## WebSocket Streaming

### Evaluation logs

**Endpoint:** `GET /api/v1/commits/:commit_id/eval/stream` (WebSocket upgrade)

**Purpose:** Stream evaluation logs while `nix-eval-jobs` runs. Handler: `stream_eval_logs` in `handlers/api/commits.rs`.

**Authorization:** A valid session with the Viewer, Operator, or Admin role. Otherwise the server answers HTTP 403 before the upgrade.

**Protocol:**

1. The client opens the WebSocket for a commit ID.
2. The server replays buffered history, up to 2000 messages per commit (`EVAL_LOG_HISTORY_BUFFER`).
3. The server then streams live messages and sends a ping every 20 seconds.
4. When 1024 evaluation channels already exist (`MAX_EVAL_LOG_CHANNELS`), the server closes with code 1013.

The server keeps a commit's channel for 10 minutes after evaluation completes, so a late client still receives history. The server does not close the connection with a "not found" message when no evaluation runs.

**Messages.** Each message is a JSON object tagged by `type` (the `EvalLogMessage` enum):

```json
{ "type": "log", "message": "evaluating system: nixos-desktop" }
```

```json
{ "type": "system_status", "system": "nixos-desktop", "status": "evaluating" }
```

```json
{ "type": "system_status", "system": "nixos-desktop", "status": "failed", "error": "..." }
```

```json
{ "type": "eval_status", "status": "complete", "message": "..." }
```

`error` and `message` are optional and absent when empty. Messages carry no timestamp.

**`system_status` values** (`SystemEvalStatus`):

- `pending`: waiting to evaluate.
- `evaluating`: `nix-eval-jobs` is running for this system.
- `success`: evaluation succeeded.
- `failed`: evaluation failed.
- `policy_failed`: Crystal Forge is disabled for this system, so the server skips it.
- `queued_for_build`: the system passed policy and has a build job.

**Related stream.** `GET /api/v1/build-jobs/:job_id/logs/stream` streams build logs.

**Key files** (under `packages/default/crates/cf-server/src/`):

- `handlers/api/commits.rs`: WebSocket handler, `EvalLogMessage`, `SystemEvalStatus`.
- `models/evaluate_with_policies.rs`: broadcasts status updates.

## Summary

| Resource | Endpoints | Auth |
|----------|-----------|------|
| Systems | list, detail, sync, deploy, and others | Viewer+ read. Admin or Operator for mutations (`can_mutate_systems`). Environment scope applies. |
| Flakes | list, create, sync, refresh, and others | Viewer+ read. Operator+ for most mutations (`RequireOperator`), with at least one Admin-only route. |
| Builders | list, register, update, deactivate (no pause or resume route) | Admin |
| Build Queue | list, cancel, requeue, reorder | Viewer+ read. Operator+ or Admin mutate. |
| Eval Queue | read, reorder, cancel, re-evaluate | Viewer+ read. Operator+ for reorder and cancel. Admin for re-evaluate. |
| Environments | CRUD | Read is membership-scoped. Mutate is Admin. |
| Dashboard | GET `/dashboard/summary`, `/dashboard/activity` | Viewer+ |
| Admin Users | list, create, update, delete | Admin |
| Admin Audit | GET `/admin/audit-events` | Admin |
| Admin OIDC | list, upsert, delete | Admin |
| Agent and Builder | Various | Signed request |
| WebSocket | `/api/v1/commits/:commit_id/eval/stream` | Session (Viewer+) |

Per-endpoint role gates for Builders, Build Queue, Eval Queue, Environments, Dashboard, and Admin are in [builders-queues-environments-dashboard-admin-api.md](builders-queues-environments-dashboard-admin-api.md). Systems role gates are in [systems-api.md](systems-api.md).

## Related concepts

- [API authentication, sessions, and role-based authorization](../security/api-authentication-and-authorization.md)
- [Systems API](systems-api.md)
- [Flakes API and Evaluation and Flake Snapshot API](flakes-and-evaluation-snapshot-api.md)
- [Builders, queues, environments, dashboard, and admin APIs](builders-queues-environments-dashboard-admin-api.md)
- [CVE Scan Operations](cve-scan-operations-api.md)
- [Fleet CVE Triage](fleet-cve-triage-api.md)
- [Agent API (machine auth) and Cache API](agent-and-cache-api.md)
- [Adding a New API Endpoint](../operations/adding-a-backend-api-endpoint.md)
- [Web UI navigation, shared components, responsive behavior, and structure](../ui/frontend-navigation-and-shared-patterns.md)
