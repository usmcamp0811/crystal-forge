---
type: API
title: "Builders, queues, environments, dashboard, and admin APIs"
description: "Lists the builders, evaluation queue, build queue, environments, dashboard, and admin (users, audit log, OIDC mappings) endpoints with roles, states, and example payloads."
tags:
  - crystal-forge
  - api
  - builders
  - queues
  - environments
  - dashboard
  - admin
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
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/api/builders.rs at commit 3b23d36f"
    title: "builders.rs implementation"
  - id: code-3
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/api/commits.rs at commit 3b23d36f"
    title: "commits.rs implementation"
  - id: code-4
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/api/environments.rs at commit 3b23d36f"
    title: "environments.rs implementation"
  - id: code-5
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/api/admin.rs at commit 3b23d36f"
    title: "admin.rs implementation"
  - id: code-6
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/api/dashboard.rs at commit 3b23d36f"
    title: "dashboard.rs implementation"
---

# Builders, queues, environments, dashboard, and admin APIs

## Builders API

Builders are worker processes that build Nix derivations.

### Endpoints

All paths below are relative to `/api/v1` and are registered in `packages/default/crates/cf-server/src/bin/server.rs`. Every administrative builder route requires the Admin role (`require_admin`).

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/builders` | Admin | List builders |
| POST | `/builders` | Admin | Register builder |
| GET | `/builders/:id` | Admin | Get builder details |
| PATCH | `/builders/:id` | Admin | Update builder |
| DELETE | `/builders/:id` | Admin | Deactivate builder (`deactivate_builder`) |
| DELETE | `/builders/:id/permanent` | Admin | Delete builder permanently |
| PUT | `/builders/:id/public-key` | Admin | Replace builder public key |
| POST | `/builders/:id/regenerate-keypair` | Admin | Regenerate builder key pair |
| PATCH | `/builders/:id/environments` | Admin | Update builder environment assignment |
| GET | `/builders/:id/metrics` | Admin | Builder metrics |

Builder-authenticated routes (`/builders/resolve-id`, `/builders/:id/session`, `/builders/:id/heartbeat`, `/builders/:id/next-job`, `/builders/:id/jobs/...`, `/builders/:id/cve-scans/...`) use signed builder requests, not browser sessions. See [Builder request authentication](../security/builder-request-authentication-and-data-in-transit.md).

> **Status (documentation stale, corrected):** The source listed `/builders/:id/pause`, `/builders/:id/resume`, `/builders/:id/jobs`, and Viewer+ read access. None of those routes is registered, and list and get require Admin. See the verification notes.

### Builder States

| State | Meaning |
|-------|---------|
| active | Builder session established and builder heartbeating (`establish_builder_session` sets `active`) |
| inactive | Default status of a newly registered builder (`builders.status` default) |
| offline | Set by the offline-builder sweep in `queries/builders.rs` |
| draining | Allowed by the CHECK constraint and `BuilderStatus::Draining`; no code path that sets it was found |

> **Status (documentation stale, corrected):** The source listed `idle`, `building`, and `paused`. The `builders.status` CHECK constraint allows `active`, `inactive`, `offline`, and `draining` (`migrations/0083_create_builders_infrastructure.sql`, `migrations/0124_add_builder_ui_fields.sql`).

---

## Evaluation Queue API

The evaluation queue manages commit evaluations (nix-eval-jobs runs).

### Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/commits/eval-queue` | Viewer+ (environment-scoped for non-Admin) | Get evaluation queue with status |
| POST | `/commits/eval-queue/reorder` | Operator+ | Change queue order |
| GET | `/commits/eval-history` | Viewer+ | Terminal evaluation history |
| POST | `/commits/:commit_id/re-evaluate` | Admin | Re-queue a commit evaluation |
| POST | `/commits/:commit_id/cancel-evaluation` | Operator+ | Request cooperative cancellation |
| POST | `/commits/:commit_id/force-cancel-evaluation` | Operator+ | Force cancellation |
| GET | `/commits/:commit_id/eval/stream` | Viewer+ | Live evaluation log stream |
| GET | `/commits/:commit_id/eval/logs` | Viewer+ | Stored evaluation log history |

> **Status (documentation stale, response shape):** The example response below is the source's original shape. The registered handler `list_eval_queue` (`handlers/api/commits.rs`) returns `EvalQueueSummary` (`api/models.rs`): `active_count`, `completed_count`, `successful_count`, `failed_count`, `domain_total`, `filtered_total`, `execution_mode`, and `items[]` (`commit_id`, `flake_id`, `flake_name`, `branch`, `commit_hash`, `commit_message`, `author`, `committed_at`, `enqueued_at`, `is_latest_per_flake`, `evaluation_status`, `queue_position`, `systems`, ...). Query parameters are `limit`, `status`, `flake`, `search`, and `latest_only`. The example is kept as the historical source text. The full item field list was not compared.

### GET /commits/eval-queue

**Response (original source example, superseded by the status note above):**
```json
{
  "active_queue": [
    {
      "commit_id": 123,
      "flake_id": 1,
      "flake_name": "nixos-configs",
      "git_commit_hash": "abc123...",
      "commit_message": "Update system configs",
      "commit_timestamp": "2024-03-02T12:00:00Z",
      "evaluation_status": "in_progress",
      "eval_queue_position": 1,
      "system_statuses": [
        {
          "system_name": "nixos-desktop",
          "status": "evaluating"
        },
        {
          "system_name": "nixos-server",
          "status": "policy_passed"
        }
      ]
    }
  ],
  "completed_queue": [...]
}
```

### POST /commits/eval-queue/reorder

**Request (registered shape, `ReorderEvalQueueRequest`):**
```json
{
  "ordered_commit_ids": [123, 124, 125]
}
```

> **Status (documentation stale, corrected):** The source described `{"commit_id": 123, "new_position": 2}`, which moves one commit. The handler accepts `ordered_commit_ids`, a complete ordered list, and returns `400` for an invalid reorder request (`handlers/api/commits.rs`, `reorder_eval_queue`).

### Evaluation States

```
pending → in_progress → complete
            ↓
          failed
```

> **Status (documentation stale):** The `commits_evaluation_status_check` constraint also allows `cancelling` and `cancelled` (`migrations/0113_add_eval_cancellation_support.sql`).

**Per-System States** (during in_progress):
```
pending → evaluating → eval_complete → policy_check
                 ↓              ↓
            eval_failed    policy_passed / policy_failed
```

**Key Invariant:** Only ONE commit can have `evaluation_status = 'in_progress'` at a time. The unique index `idx_commits_single_in_progress` enforces this across `in_progress` and `cancelling` (`migrations/0113_add_eval_cancellation_support.sql`).

---

## Build Queue API

The build queue manages Nix derivation builds.

### Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/build-jobs` | Viewer+ | Get a bounded page of pending/in-progress builds |
| GET | `/build-jobs/recent` | Viewer+ | Get a bounded page of terminal build attempts |
| GET | `/build-jobs/:id` | Viewer+ | Get one exact visible attempt and its active or completed collection |
| POST | `/build-jobs/:id/cancel` | Admin | Cancel a build job |
| POST | `/build-jobs/:id/force-cancel` | Admin | Force-cancel a build job |
| POST | `/build-jobs/:id/requeue` | Operator+ | Re-queue a build job |
| POST | `/build-jobs/:id/prioritize` | Operator+ | Prioritize a queued job |
| POST | `/build-jobs/:id/move-up` | Operator+ | Move a queued job up |
| POST | `/build-jobs/:id/move-down` | Operator+ | Move a queued job down |
| POST | `/build-queue/reorder` | Operator+ | Reorder the queue (`{"ordered_job_ids": [...]}`) |
| GET | `/build-jobs/:job_id/logs/stream` | Viewer+ | Live build log stream |

> **Status (documentation stale, corrected):** The source listed `POST /build-queue` (queue a derivation) and `DELETE /build-queue/:id`. Neither is registered. No registered route queues a derivation directly. Non-Admin callers see only jobs in their environment memberships (`visibility_user_id` in `list_build_queue`).

The exact build-attempt endpoint applies the caller's environment visibility in
the primary-key query. It returns `404 Not Found` for both missing attempts and
attempts outside the caller's visibility scope. This behavior prevents attempt
identity disclosure. Exact lookup does not expand either paginated list and does
not treat a UUID as ordinary text search.

### Build States

```
pending → building → built → cache-pushing → cache-pushed
            ↓            ↓           ↓
          failed    cache-failed  cache-failed
```

> **Status (documentation stale, partially checked):** The `build_jobs.status` CHECK constraint allows `queued`, `building`, `cancelling`, `cancelled`, `success`, and `failed` (`migrations/0103_expand_build_job_status_for_cancellation.sql`). The diagram above is the source text. The server also registers `/api/v1/cache-push-jobs` routes for cache push work. The diagram state names were not mapped to code.

---

## Environments API

Environments group systems logically.

### Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/environments` | Authenticated (membership-scoped for non-Admin) | List environments |
| POST | `/environments` | Admin | Create environment |
| GET | `/environments/:id` | Authenticated (membership-scoped for non-Admin) | Get environment |
| PATCH | `/environments/:id` | Admin | Update environment |
| DELETE | `/environments/:id` | Admin | Delete environment (`409` while systems are assigned) |
| GET | `/environments/:id/policies` | Authenticated (membership-scoped) | Get environment with required policies |
| PATCH | `/environments/:id/policies` | Admin | Replace required policy IDs |
| GET | `/environments/policies-map` | Authenticated (membership-scoped) | Environment-to-policy map |
| GET | `/policies` | Authenticated | List policies |

The environment handlers use `authenticated_user_roles`, `highest_role`, and `Role::can_manage_environments` (Admin only) in `handlers/api/environments.rs`. The Viewer+ gate of the other endpoints was checked for the list/get handlers only through the membership filter; `list_policies_handler` was not read.

---

## Dashboard API

Aggregated fleet data.

### Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/dashboard/summary` | Viewer+ (environment-scoped for non-Admin) | Fleet summary |
| GET | `/dashboard/activity` | Viewer+ (environment-scoped for non-Admin) | Recent activity |

> **Status (documentation stale, corrected):** The source listed `/dashboard`, `/dashboard/builds`, and `/dashboard/flakes`. Only `/api/v1/dashboard/summary` and `/api/v1/dashboard/activity` are registered. The CVE dashboard routes (`/cves/summary`, `/cves/vulnerabilities`, `/cves/top-systems`, `/cves/scan-freshness`) are Admin-only (`handlers/api/dashboard.rs`).

### Example Response (original source example; the registered response shape was not compared)

```json
{
  "data": {
    "systems": {
      "total": 10,
      "online": 8,
      "offline": 2
    },
    "environments": {
      "production": 5,
      "staging": 3,
      "development": 2
    },
    "builds": {
      "pending": 3,
      "building": 1,
      "recent": [...]
    }
  }
}
```

---

## Admin API

Admin-only endpoints for user and system management.

### Users Management

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/admin/users` | Admin | List users |
| POST | `/admin/users` | Admin | Create user |
| PATCH | `/admin/users/:id` | Admin | Update user |
| DELETE | `/admin/users/:id` | Admin | Delete user |

### Audit Log

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/admin/audit-events` | Admin | List audit events |

**Query Parameters (`AuditEventsQuery`):**
```bash
GET /api/v1/admin/audit-events?from=2024-01-01&to=2024-01-31&actor=john&action=...&page=1&per_page=50
```

### OIDC Mappings

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/admin/oidc-mappings` | Admin | List mappings |
| POST | `/admin/oidc-mappings` | Admin | Create or update a mapping by group name (`upsert_oidc_mapping`) |
| DELETE | `/admin/oidc-mappings/:id` | Admin | Delete mapping |

### Other registered admin routes

`GET /admin/server-info`, `GET`/`PUT /admin/classification-config`, `GET`/`PUT /admin/automatic-retry-policy`, `GET /admin/setup-progress`, `POST /admin/setup-wizard/dismiss`, `POST /admin/setup-wizard/agent-acknowledge`, and `GET /admin/config-health`. These were not in the source and were not read beyond their `require_admin` gates for `server-info`, `classification-config`, and `automatic-retry-policy`.

> **Status (documentation stale, corrected):** The source routes `/admin/users/:id` (GET), `/admin/audit` and `/admin/audit/export`, and `PATCH /admin/oidc-mappings/:id` are not registered in `packages/default/crates/cf-server/src/bin/server.rs`. The audit query parameters are `from`, `to`, `actor`, `action`, `page`, and `per_page`, not `start_date` and `end_date`. The role label `Admin+` in the source means Admin; no higher role exists (`AuthRole` has Admin, Operator, and Viewer).

## Related concepts

- [Backend API overview, error codes, and WebSocket streaming](api-overview-errors-and-streaming.md)
- [Systems API](systems-api.md)
- [Fleet CVE Triage](fleet-cve-triage-api.md)
- [API authentication, sessions, and role-based authorization](../security/api-authentication-and-authorization.md)

## Migration verification notes

Checked against commit `3b23d36f`: route registration, role gates of builders, build-queue, eval-queue, environments, dashboard, and admin handlers. Not checked: response payload shapes other than the eval queue, dashboard summary shape, and the `Viewer+` gate of `list_policies_handler`. No `verified` field is set because the response examples were not compared.

- Claim: builders API exposes Viewer+ list/get, `pause`, `resume`, and `/builders/:id/jobs`.
  Finding: list/get require Admin; no pause, resume, or jobs route exists; extra routes (`permanent`, `public-key`, `regenerate-keypair`, `environments`, `metrics`) exist.
  Evidence: `bin/server.rs` route table; `handlers/api/builders.rs` (`require_admin`).
  Case: documentation stale.
- Claim: builder states are idle/building/paused.
  Finding: CHECK allows active/inactive/offline/draining.
  Evidence: `migrations/0083_create_builders_infrastructure.sql`, `0124_add_builder_ui_fields.sql`; `queries/builders.rs` `mark_stale_builders_offline`.
  Case: documentation stale.
- Claim: `POST /commits/eval-queue/reorder` takes `{commit_id, new_position}`.
  Finding: it takes `ordered_commit_ids`.
  Evidence: `api/models.rs` `ReorderEvalQueueRequest`; `handlers/api/commits.rs`.
  Case: documentation stale.
- Claim: eval queue response is `active_queue`/`completed_queue`.
  Finding: response is `EvalQueueSummary` with `items`.
  Evidence: `api/models.rs`.
  Case: documentation stale.
- Claim: `POST /build-queue`, `DELETE /build-queue/:id`.
  Finding: not registered; `/build-jobs/:id/{cancel,force-cancel,requeue,prioritize,move-up,move-down}` and `/build-queue/reorder` are registered.
  Evidence: `bin/server.rs`; `handlers/api/builders.rs`.
  Case: documentation stale.
- Claim: single in-progress evaluation invariant.
  Finding: holds, unique index spans `in_progress` and `cancelling`.
  Evidence: `migrations/0113_add_eval_cancellation_support.sql`.
  Case: implemented.
- Claim: dashboard routes `/dashboard`, `/dashboard/builds`, `/dashboard/flakes`.
  Finding: only `/dashboard/summary` and `/dashboard/activity`.
  Evidence: `bin/server.rs`; `handlers/api/dashboard.rs`.
  Case: documentation stale.
- Claim: admin routes `/admin/audit`, `/admin/audit/export`, `GET /admin/users/:id`, `PATCH /admin/oidc-mappings/:id`.
  Finding: not registered; `/admin/audit-events` with `from`/`to`/`actor`/`action`/`page`/`per_page` is registered; OIDC mappings use POST upsert.
  Evidence: `bin/server.rs`; `handlers/api/admin.rs` (`AuditEventsQuery`, `upsert_oidc_mapping`).
  Case: documentation stale.
- Claim: exact build-attempt lookup returns 404 for invisible attempts.
  Finding: not checked in the query; handler applies `visibility_user_id` for non-Admin.
  Evidence: `handlers/api/builders.rs` `get_build_attempt`.
  Case: not fully checked.
