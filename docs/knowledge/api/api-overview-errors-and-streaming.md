---
type: API
title: "Backend API overview, error codes, and WebSocket streaming"
description: "Describes the REST base URL, request and response envelopes, common error codes, the evaluation-log WebSocket stream, and the per-resource endpoint summary table; open it first when working with the Crystal Forge HTTP API."
tags:
  - crystal-forge
  - api
  - rest
  - websocket
  - errors
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:29-05:00
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

This document describes Crystal Forge's HTTP API. It's written for developers who need to understand how the backend works, what endpoints exist, and how to add new ones.

**Assumption:** You understand HTTP (GET, POST, etc.), REST APIs, and basic database concepts.

## API Overview

The API is a **REST API** that the frontend uses to talk to the backend.

**Base URL:** `http://localhost:8080/api/v1/`

### Request Format

- **Headers:** `Content-Type: application/json`
- **Body:** JSON for POST/PATCH requests
- **Authentication:** Cookie-based sessions

### Response Format

**Success:**
```json
{
  "data": {
    "id": "123",
    "name": "example"
  }
}
```

**Paginated:**
```json
{
  "data": [...],
  "pagination": {
    "page": 1,
    "per_page": 20,
    "total": 100
  }
}
```

**Error:**
```json
{
  "error": {
    "code": "NOT_FOUND",
    "message": "System not found"
  }
}
```

> **Status:** The Base URL above uses port 8080. The NixOS module default for the server port is 3000 (`services.crystal-forge.server.port` in `modules/nixos/crystal-forge/default.nix`). The port is deployment configuration, so 8080 is an example, not a fixed default.

> **Status (documentation stale, corrected):** The `{"data": ...}` success envelope, the `pagination.page/per_page/total` envelope, and the `{"error": {"code": "NOT_FOUND", ...}}` error envelope above are the source text and do not match the registered handlers. Handlers return the response DTO directly with no `data` wrapper (for example `queue_system_config_inspection` returns `Json(response)`), and each paginated endpoint defines its own pagination fields in `api/models.rs`. Errors use the flat `ApiError` DTO: `{"error": "<lowercase_code>", "message": "...", "details": <optional>}` (`api/models.rs`, `ApiError`). Observed codes include `forbidden`, `not_found`, `validation_error`, `conflict`, and `internal_error`; some compliance handlers use upper-case domain codes such as `POLICY_INTERCHANGE_INVALID` and `ASSIGNMENT_TARGET_NOT_FOUND`.

## Common Error Codes

| Code | Meaning | When Used |
|------|---------|-----------|
| UNAUTHORIZED | No valid session | Not logged in |
| FORBIDDEN | Insufficient permissions | Logged in but wrong role |
| NOT_FOUND | Resource doesn't exist | ID is wrong |
| VALIDATION_ERROR | Invalid input | Bad request data |
| CONFLICT | Resource already exists | Duplicate create |

> **Status (documentation stale, corrected):** The registered handlers emit lowercase codes `forbidden`, `not_found`, `validation_error` (HTTP 400), `conflict` (HTTP 409), and `internal_error`. The API-handler authentication helpers (`authenticated_user_roles`, `require_*` in `handlers/api/rbac.rs`) return `None` for a missing, expired, or invalidated session, and the handlers that use them (builders, commits, environments, dashboard, admin, systems) respond with HTTP 403 `forbidden`. HTTP 401 (`StatusCode::UNAUTHORIZED`) is used by the signed agent and builder request verification (`handlers/agent_request.rs`, `handlers/builder_request.rs`), and by `handlers/api/{caches,user_sessions,deployments,auth_local,auth_oidc}.rs`. Handlers deliberately return `not_found` for resources hidden by environment scope.
## WebSocket Streaming

### Evaluation Logs (Real-Time)

**Endpoint:** `ws://localhost:8080/ws/eval-stream/:commit_id`

**Purpose:** Stream evaluation logs in real-time as nix-eval-jobs runs.

**Protocol:**
1. Client connects with commit ID
2. Server checks if commit evaluation is in progress
3. If yes: streams log lines as they appear
4. If no: closes connection with "not found" message

**Message Format:**
```json
{
  "type": "log",
  "data": "evaluating system: nixos-desktop",
  "timestamp": "2024-03-02T12:34:56Z"
}
```

**System Status Updates:**
```json
{
  "type": "system_status",
  "system": "nixos-desktop",
  "status": "evaluating",
  "data": null
}
```

```json
{
  "type": "system_status",
  "system": "nixos-desktop",
  "status": "policy_passed",
  "data": {
    "queued_for_build": true
  }
}
```

**Status Values:**
- `pending` - Waiting to evaluate
- `evaluating` - Currently running nix-eval-jobs
- `eval_complete` - Evaluation succeeded
- `eval_failed` - Evaluation failed
- `policy_passed` - CF enabled, added to build queue
- `policy_failed` - CF disabled, skipped

**Key Files:**
- `src/handlers/websocket.rs` - WebSocket handler
- `src/models/evaluate_with_policies.rs` - Broadcasts status updates

> **Status (documentation stale, corrected):** The WebSocket route is `GET /api/v1/commits/:commit_id/eval/stream` (`stream_eval_logs`, `handlers/api/commits.rs`), not `/ws/eval-stream/:commit_id`. It requires a valid session with Viewer, Operator, or Admin role (otherwise HTTP 403) and then upgrades the connection. The server replays buffered history (up to 2000 messages per commit, `EVAL_LOG_HISTORY_BUFFER`), then streams live messages, and sends a ping every 20 seconds. It closes with code 1013 when 1024 evaluation channels already exist (`MAX_EVAL_LOG_CHANNELS`). A channel is kept for 10 minutes after evaluation completes so late clients receive history (`cleanup_eval_channel`). The server does not close with a "not found" message when no evaluation is running. A second stream, `GET /api/v1/build-jobs/:job_id/logs/stream`, serves build logs. The message shapes are the tagged `EvalLogMessage` enum: `{"type":"log","message":...}`, `{"type":"system_status","system":...,"status":...,"error"?:...}`, and `{"type":"eval_status","status":...,"message"?:...}`. The `system_status` values are `pending`, `evaluating`, `success`, `failed`, `policy_failed`, and `queued_for_build` (`SystemEvalStatus`). The `timestamp` and `data` fields and the status names `eval_complete`, `eval_failed`, and `policy_passed` in the source text above are not part of the current messages. The key-file paths above are original-document paths; the crate layout is `packages/default/crates/cf-server/src/`, and no `handlers/websocket.rs` exists in the current tree.

## Summary

| Resource | Endpoints | Auth |
|----------|-----------|------|
| Systems | CRUD + deploy/rollback | Viewer+ |
| Flakes | CRUD + sync | Viewer+ |
| Builders | CRUD (no pause/resume route) | Admin |
| Build Queue | list, cancel, requeue, reorder | Viewer+ read; Operator+ or Admin mutate |
| Eval Queue | GET + reorder | Viewer+ |
| Environments | CRUD | Read membership-scoped; mutate Admin |
| Dashboard | GET (`/dashboard/summary`, `/dashboard/activity`) | Viewer+ |
| Admin Users | list/create/update/delete | Admin |
| Admin Audit | GET `/admin/audit-events` | Admin |
| Admin OIDC | list, upsert, delete | Admin |
| Agent/Builder | Various | Key-based |
| WebSocket | /api/v1/commits/:commit_id/eval/stream | Session (Viewer+) |

> **Status:** The Auth column in this table is a summary. Per-endpoint role gates for Builders, Build Queue, Eval Queue, Environments, Dashboard, and Admin are corrected in [builders-queues-environments-dashboard-admin-api.md](builders-queues-environments-dashboard-admin-api.md). The Systems and Flakes rows keep the source labels; mutation roles for those resources were not checked in this note.

For frontend views, see `01-frontend-views.md`.
For system overview, see `00-system-overview.md`.

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

## Migration verification notes

Checked against commit `3b23d36f`: `bin/server.rs` route registration, `handlers/api/commits.rs` WebSocket handler and message types, `api/models.rs` `ApiError`, `handlers/api/rbac.rs`, and the Admin/Viewer gates listed in the Summary rows for builders, commits, environments, dashboard, and admin. Not checked: Systems and Flakes mutation roles, any paginated response shape, and `Content-Type` handling. No `verified` field is set.

- Claim: success responses are wrapped in `{"data": ...}` and paginated responses use `pagination.page/per_page/total`.
  Finding: handlers return DTOs directly; pagination fields are defined per endpoint.
  Evidence: `handlers/api/systems.rs` `queue_system_config_inspection`; `api/models.rs`.
  Case: documentation stale.
- Claim: errors are `{"error": {"code": "NOT_FOUND", "message": ...}}` with upper-case codes including UNAUTHORIZED.
  Finding: flat `ApiError {error, message, details?}` with lowercase codes; the session helpers lead to HTTP 403, while signed agent/builder verification and some handlers emit 401.
  Evidence: `api/models.rs` `ApiError`; `handlers/api/environments.rs`; `handlers/api/rbac.rs`.
  Case: documentation stale.
- Claim: WebSocket `ws://.../ws/eval-stream/:commit_id` with `type: log` messages carrying `timestamp`.
  Finding: route is `/api/v1/commits/:commit_id/eval/stream`; message types are `log`, `system_status`, `eval_status`; history replay and ping exist.
  Evidence: `bin/server.rs`; `handlers/api/commits.rs` (`EvalLogMessage`, `SystemEvalStatus`, `handle_eval_stream`).
  Case: documentation stale.
- Claim: `Key Files: src/handlers/websocket.rs`.
  Finding: the file does not exist in the current tree.
  Evidence: `cf-server/src/handlers/` listing.
  Case: documentation stale.
- Claim: Base URL port 8080.
  Finding: port is configuration; the NixOS module default is 3000.
  Evidence: `modules/nixos/crystal-forge/default.nix`.
  Case: documentation stale (example value only).
- Claim: Builders/Environments/Admin summary roles.
  Finding: corrected in the Summary table; builders list/get are Admin only.
  Evidence: `handlers/api/builders.rs`; `handlers/api/environments.rs`; `handlers/api/admin.rs`.
  Case: documentation stale.
