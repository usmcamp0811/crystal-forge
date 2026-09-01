# Backend API Specification

This document describes Crystal Forge's HTTP API. It's written for developers who need to understand how the backend works, what endpoints exist, and how to add new ones.

**Assumption:** You understand HTTP (GET, POST, etc.), REST APIs, and basic database concepts.

---

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

---

## Authentication

### How Sessions Work

1. User logs in (OIDC or Dev Mode)
2. Server creates session in database
3. Server sets `session_id` cookie in browser
4. Subsequent requests include the cookie
5. Middleware validates session

### Login Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/auth/login` | Local email/password login |
| POST | `/auth/logout` | Clear session |
| GET | `/auth/status` | Get current user info |
| POST | `/dev/login` | Dev mode role selection |

### Dev Mode

For local development without OIDC:

```bash
# After setting AUTH_MODE=dev
curl -X POST http://localhost:8080/api/v1/dev/login \
  -H "Content-Type: application/json" \
  -d '{"role": "admin"}'
```

Returns a session cookie.

---

## Authorization (RBAC)

### Roles

| Role | What They Can Do |
|------|------------------|
| **Viewer** | Read-only access to everything |
| **Operator** | Deploy, rollback, sync flakes, manage systems |
| **Admin** | All of above + user management, audit log |

### Authorization Middleware

Every protected endpoint uses middleware to check permissions:

```rust
// Example: Operator or Admin only
async fn handler(
    State(state): State<AppState>,
    Session(user): Session,  // Gets current user from cookie
) -> Result<Json<...>, Error> {
    // Check role
    if user.role == "viewer" {
        return Err(Error::forbidden("Viewers cannot do this"));
    }
    // ... handler logic
}
```

### Environment Scoping

Users can only see **systems in their assigned environments**.

```sql
-- Query includes WHERE environment_id IN (user's environments)
SELECT * FROM systems 
WHERE environment_id IN (
  SELECT environment_id 
  FROM user_environment_memberships 
  WHERE user_id = ?
);
```

**Exception:** Admins can see all systems regardless of environment.

Snapshot APIs preserve non-disclosure. An unknown resource, a resource in a
hidden environment, and a revision outside the resource's active source use the
same not-found response. See [Evaluation and Flake Snapshot
Architecture](../evaluation-flake-snapshots.md#flake-outputs-and-count-authority).

---

## Systems API

Systems are the NixOS machines CF manages.

### Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/systems` | Viewer+ | List all systems |
| POST | `/systems` | Operator+ | Register new system |
| GET | `/systems/:id` | Viewer+ | Get system details |
| PATCH | `/systems/:id` | Operator+ | Update system |
| DELETE | `/systems/:id` | Admin+ | Remove system |
| POST | `/systems/:id/deploy` | Operator+ | Trigger deployment |
| POST | `/systems/:id/rollback` | Operator+ | Rollback generation |
| POST | `/systems/:id/sync` | Operator+ | Sync flake |
| GET | `/systems/:id/deployments` | Viewer+ | Deployment history |
| GET | `/systems/:id/logs` | Viewer+ | Deployment logs |
| GET | `/systems/:id/evaluated-options` | Viewer+ | Read cached revision options |
| GET | `/systems/:id/evaluation-summary` | Viewer+ | Read cached scalar revision summary |
| GET | `/systems/:id/evaluation-module-sources` | Viewer+ | Read cached bounded module-source pages |
| POST | `/systems/:id/evaluations/:revision` | Admin | Queue or reuse evaluation |

### Query Parameters

```bash
# Filter by environment
GET /api/v1/systems?environment=prod

# Filter by status
GET /api/v1/systems?status=online

# Search by name
GET /api/v1/systems?search=web
```

### Example: List Systems

**Request:**
```bash
GET /api/v1/systems?environment=prod
```

**Response:**
```json
{
  "data": [
    {
      "id": "sys-123",
      "name": "prod-web-01",
      "hostname": "prod-web-01.example.com",
      "environment_id": "env-prod",
      "environment_name": "Production",
      "status": "online",
      "last_heartbeat": "2024-01-15T10:30:00Z",
      "deployed_flake": "github:org/configs",
      "deployed_commit": "abc1234"
    }
  ],
  "pagination": {
    "page": 1,
    "per_page": 20,
    "total": 5
  }
}
```

### Example: Trigger Deployment

**Request:**
```bash
POST /api/v1/systems/sys-123/deploy
{
  "commit_sha": "def56789abcdef0123456789abcdef0123456789",
  "action": "convert_to_manual",
  "request_id": "7ce63e03-935d-4903-ae9f-903f14242cab"
}
```

Manual deployment requests also accept `action` and `request_id`. The action is
`deploy`, `continue_auto_latest`, or `convert_to_manual`. An `auto_latest`
system requires one of the latter two explicit outcomes. New clients reuse one
UUID `request_id` for retries. The UUID is bound immutably to the system, full
commit SHA, and action. A conflicting reuse returns HTTP 409 before policy
conversion. If conversion succeeds but queueing fails, the response reports the
persisted manual policy separately from the failed deployment state. Legacy
clients that omit `request_id` use a server-derived 24-hour replay window.

**Response:**
```json
{
  "status": "accepted",
  "policy": "manual",
  "conversion": "converted",
  "deployment": "queued",
  "deployment_id": "79ea0220-5715-49ce-8e73-74c09a5ea289",
  "message": "System policy is manual. Deployment requested"
}
```

---

## Flakes API

Flakes are git repositories that contain NixOS configurations.

### Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/flakes` | Viewer+ | List registered flakes |
| POST | `/flakes` | Operator+ | Add flake to registry |
| GET | `/flakes/:id` | Viewer+ | Get flake details |
| PATCH | `/flakes/:id` | Operator+ | Update flake |
| DELETE | `/flakes/:id` | Operator+ | Remove from registry |
| POST | `/flakes/:id/sync` | Operator+ | Trigger git sync |
| GET | `/flakes/:id/commits` | Viewer+ | Get commit timeline |
| GET | `/flakes/:id/revisions/:revision/outputs` | Viewer+ | Read cached revision outputs |
| GET | `/flakes/:id/revisions/:revision/modules/:module/declarations` | Viewer+ | Read cached exported-module declarations |

### Example: Get Commit Timeline

**Request:**
```bash
GET /api/v1/flakes/flake-456/commits
```

**Response:**
```json
{
  "data": [
    {
      "sha": "abc1234def5678",
      "sha_short": "abc1234",
      "message": "Update nginx config",
      "author": "john@example.com",
      "date": "2024-01-15T10:00:00Z",
      "changed_files": 2
    }
  ]
}
```

## Evaluation and Flake Snapshot API

These endpoints read persisted snapshots only. GET requests do not evaluate
Nix, inspect Git, fetch repositories, enqueue work, or perform per-host work.
All `revision` values are complete 40- or 64-character hexadecimal SHAs.

### GET `/systems/:id/evaluated-options`

Query parameters:

| Parameter | Contract |
| --- | --- |
| `revision` | Required full SHA in commit mode. |
| `mode` | `commit` or `generation`; defaults to `commit`. |
| `generation` | Required retained generation number in generation mode. |
| `search` | Case-insensitive redacted search text; truncated to 256 characters. |
| `filter` | `all`, `overridden`, or `changed`. |
| `limit` | Clamped to 1-100; defaults to 50. |
| `offset` | Clamped to 0-100,000; defaults to 0. |

The response lifecycle is `queued`, `running`, `failed`, `available`, or
`unavailable`. `counts` is revision-global and independent of search/filter.
`total` is the number of rows for the active search/filter. Changed data and
`counts.changed` are absent when no valid first-parent or preceding retained
generation snapshot exists. `module_count` is the exact count of distinct
`(source_input, source_revision, source_path)` tuples after redaction and
per-option bounding; it is not derived from the bounded option page.

### GET `/systems/:id/evaluation-summary`

This endpoint uses the same `revision`, `mode`, and `generation` selection and
non-disclosing system authorization as evaluated-options. The response is
scalar. It does not contain module-source or definition rows.

The response returns lifecycle, safe error, persisted completion time,
evaluation duration, option total, `module_source_total`, exact selected NixOS
toplevel store path, existing closure package count, exact latest running store
path, agent-reported profile match, and drift. `module_source_total` is the exact
count of distinct `(source_input, source_revision, source_path)` tuples after
redaction and per-option bounding. Response-only tracked identities do not
affect the count. Drift is `matches` only when selected and running store paths
are exactly equal, `differs` only when both paths exist and differ, and
`unavailable` otherwise.

`host_delta_count` is materialized from all usable configuration snapshots at
the selected commit. For each option path, the server selects the most frequent
complete safe content digest, including definition provenance; missing is also
a state, and bytewise state identity breaks ties. The count is the selected
snapshot's differences from that modal corpus. A usable one-configuration
corpus returns zero. Null means no usable materialized result exists.

`closure_size_bytes` is the sum of `narSize` for every unique store path from
one successful complete recursive Nix query of the selected toplevel output.
Null means no complete local measurement was persisted. The server does not
substitute derivation size, snapshot size, or a partial query.

`agent_fingerprint` compares the exact selected and latest agent-reported store
paths. It is `matches`, `differs`, or `unavailable` when either path is absent.
`seven_day_drift` is `no_observed_drift` or `observed_drift` only when persisted
state and heartbeat observations span the full trailing seven days, every
boundary or adjacent gap is at most four hours, and all observations have an
exact store path. The observation before the window establishes coverage but
does not contribute drift. Otherwise it is `insufficient_coverage`.

Completion time, duration, selected and running paths, closure counts, profile
match, and other optional facts are null when their named persisted source is
absent. A non-available lifecycle returns no summary facts and zero totals.
Clients MUST render unavailable states. They MUST NOT infer one metric from
another field or replace null, unavailable, failed, or insufficient coverage
with zero or success.

### GET `/systems/:id/evaluation-module-sources`

This endpoint uses the same selected-revision and non-disclosure contract as
evaluated-options.

| Parameter | Contract |
| --- | --- |
| `revision` | Required full SHA in commit mode. |
| `mode` | `commit` or `generation`; defaults to `commit`. |
| `generation` | Required retained generation number in generation mode. |
| `limit` | Clamped to 1-100; defaults to 50. |
| `offset` | Clamped to 0-100,000; defaults to 0. |
| `snapshot_token` | Optional on offset 0; required and a 64-character hexadecimal digest when `offset` is greater than 0. |

The response lifecycle is `queued`, `running`, `failed`, `available`, or
`unavailable`. Non-available responses contain no token or rows and return a
zero total. An available response returns one bounded page and a
snapshot-version token. `total` is the exact complete-snapshot tuple count even
when `sources` is empty because the offset is past the final row.

Rows are ordered by `won_count` descending, `defined_count` descending, then
`source_input`, `source_revision`, and `source_path` in ascending bytewise
order. Null input and revision values sort last. Each row contains the exact
tuple, snapshot-wide counts for that tuple, and optional server-issued
`tracked_flake` identity.

The first request omits `snapshot_token`. Every continuation request sends the
token from the first page. If the persisted snapshot is replaced, the endpoint
returns HTTP 409 with `snapshot_changed` and no rows. The client discards all
loaded rows and restarts at offset 0.

`tracked_flake` is response-only and is never persisted in evaluator content.
For `self`, the source revision must equal the page's exact active context
revision. For an external input, the context revision's persisted lock snapshot
must match the exact input name, repository URL, and full locked revision. The
identity is returned only when this mapping resolves unambiguously to one
non-deleted registered flake and non-archived commit visible through an active
managed system. Hidden, stale, unmatched, deleted, archived, and ambiguous
identities remain absent. Repository URLs are sanitized before serialization.

The same response-only resolver decorates every selected and baseline
definition returned by `/evaluated-options`, using the selected or baseline
revision as that definition's context. The browser independently loads summary,
module-source, and option pages. It MUST NOT infer identities or derive a
snapshot-wide module count from a bounded page.

This GET is database-only. It does not evaluate Nix, inspect Git, fetch a
repository, enqueue work, mutate snapshot state, or perform per-host work.

### POST `/systems/:id/evaluations/:revision`

This mutation requires administrator authority because the evaluator processes
the complete commit. It queues a missing terminal evaluation or reuses
available, queued, or running work. The `queued` response field is true only
when this request performed the queue transition.

### GET `/flakes/:id/revisions/:revision/outputs`

Query parameters:

| Parameter | Contract |
| --- | --- |
| `system_filter` | `all`, `declared_unmanaged`, or `managed_undeclared`; defaults to `all`. |
| `limit` | Clamped to 1-100; applies independently to each top-level collection and to filtered reconciliation. |
| `offset` | Clamped to 0-100,000; applies independently to each top-level collection and to filtered reconciliation. |
| `snapshot_token` | Optional opaque token returned by the endpoint. When supplied, it binds the request to the selected output and usable first-parent comparison state. |

The server applies `system_filter` before the reconciliation offset and limit.
`pagination.system_total` is the visible total for the active filter, and
`pagination.systems_has_more` reports whether that filtered sequence has a next
row. The aggregate reconciliation counts, collapse count, pinned count, and
stale-input count remain revision-global. Clients request continuation pages
and retain these authoritative totals. A response larger than the 2 MiB safe
response bound is `unavailable` rather than silently truncated.

Token-aware clients send the first page's `snapshot_token` on continuation
requests. The server returns `409 snapshot_changed` if a supplied token is
stale or malformed because the selected output, first-parent identity or state,
or usable first-parent output changed. The client then discards accumulated
rows and restarts at offset 0. For compatibility with existing clients, a
positive offset without `snapshot_token` retains the prior bounded offset
semantics and does not receive this replacement guarantee. HTTP 409 applies
only when the request supplied a stale or malformed token.

`managed_system_count` is the authoritative visible active fleet count. It can
exceed the bounded `systems` array. Non-admin responses remove hidden systems,
configuration names, and module consumers. A caller without a visible active
managed system for the flake receives not-found.

Exported-module entries in this response are summaries. `declaration_count`
remains authoritative. `declarations` is empty, and `declarations_complete` is
false when declaration details exist. Clients use the dedicated declaration
endpoint instead of treating the summary as a complete nested collection.

An exported module's `source_input`, `source_revision`, and `source_path`
describe only the location of its `nixosModules` attribute binding. The
evaluator uses the Nix attribute position and requires one unambiguous longest
matching input root; `source_path` is relative to that root. Missing positions
and ambiguous roots produce null. These fields are not module value provenance
and do not authorize navigation. Declaration `source_paths` are the declaration
locations.

Input rows expose `direct_descendant_count` for immediate lock-graph children
and `transitive_descendant_count` for all unique recursive descendants of a
direct root input. Both counts use the complete lock graph, not the response
page. They are null for non-direct nodes or unavailable counts. Clients that
describe transitive reach MUST use `transitive_descendant_count`.

### GET `/flakes/:id/revisions/:revision/modules/:module/declarations`

This endpoint returns declarations for one exact exported module from one
persisted flake-output JSONB snapshot. `limit` is clamped to 1-100 and `offset`
to 0-100,000. The response contains the authoritative `total`, applied
`offset` and `limit`, deterministic declaration rows, explicit snapshot
`lifecycle` and safe `error`, and a content-digest `snapshot_token`.

The first request omits `snapshot_token`. Every continuation request sends the
token returned by page one. If re-evaluation replaces the selected snapshot,
the endpoint returns `409 snapshot_changed`. The client must discard loaded
rows and restart at offset 0. Unknown active revisions and module names return
not-found. Unauthorized or hidden flakes use the same non-disclosing behavior
as the top-level output endpoint. The query is database-only and does not
mutate evaluation or snapshot state.

See [Evaluation and Flake Snapshot
Architecture](../evaluation-flake-snapshots.md) for extraction ownership,
identity, comparison, persistence, retention, redaction, and verification
requirements.

---

## Builders API

Builders are worker processes that build Nix derivations.

### Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/builders` | Viewer+ | List builders |
| POST | `/builders` | Admin+ | Register builder |
| GET | `/builders/:id` | Viewer+ | Get builder details |
| PATCH | `/builders/:id` | Admin+ | Update builder |
| DELETE | `/builders/:id` | Admin+ | Remove builder |
| POST | `/builders/:id/pause` | Admin+ | Pause builder |
| POST | `/builders/:id/resume` | Admin+ | Resume builder |
| GET | `/builders/:id/jobs` | Viewer+ | Builder's job history |

### Builder States

| State | Meaning |
|-------|---------|
| idle | Waiting for work |
| building | Currently building |
| paused | Admin paused |

---

## Evaluation Queue API

The evaluation queue manages commit evaluations (nix-eval-jobs runs).

### Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/commits/eval-queue` | Viewer+ | Get evaluation queue with status |
| POST | `/commits/eval-queue/reorder` | Operator+ | Change queue order |

### GET /commits/eval-queue

**Response:**
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

**Request:**
```json
{
  "commit_id": 123,
  "new_position": 2
}
```

Moves the specified commit to the given position in the queue. Queue positions are recalculated for all affected commits.

### Evaluation States

```
pending → in_progress → complete
            ↓
          failed
```

**Per-System States** (during in_progress):
```
pending → evaluating → eval_complete → policy_check
                 ↓              ↓
            eval_failed    policy_passed / policy_failed
```

**Key Invariant:** Only ONE commit can have `evaluation_status = 'in_progress'` at a time.

---

## Build Queue API

The build queue manages Nix derivation builds.

### Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/build-queue` | Viewer+ | Get pending/in-progress builds |
| POST | `/build-queue` | Operator+ | Queue new derivation |
| GET | `/build-queue/:id` | Viewer+ | Get build status |
| DELETE | `/build-queue/:id` | Operator+ | Cancel pending build |

### Build States

```
pending → building → built → cache-pushing → cache-pushed
           ↓            ↓           ↓
         failed    cache-failed  cache-failed
```

---

## Environments API

Environments group systems logically.

### Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/environments` | Viewer+ | List environments |
| POST | `/environments` | Admin+ | Create environment |
| GET | `/environments/:id` | Viewer+ | Get environment |
| PATCH | `/environments/:id` | Admin+ | Update environment |
| DELETE | `/environments/:id` | Admin+ | Delete environment |

---

## Dashboard API

Aggregated fleet data.

### Endpoints

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/dashboard` | Viewer+ | Fleet summary |
| GET | `/dashboard/builds` | Viewer+ | Build queue summary |
| GET | `/dashboard/flakes` | Viewer+ | Flake sync status |

### Example Response

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
| GET | `/admin/users` | Admin+ | List users |
| POST | `/admin/users` | Admin+ | Create user |
| GET | `/admin/users/:id` | Admin+ | Get user |
| PATCH | `/admin/users/:id` | Admin+ | Update user |
| DELETE | `/admin/users/:id` | Admin+ | Delete user |

### Audit Log

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/admin/audit` | Admin+ | List audit events |
| GET | `/admin/audit/export` | Admin+ | Export audit log |

**Query Parameters:**
```bash
GET /api/v1/admin/audit?start_date=2024-01-01&end_date=2024-01-31&actor=john
```

### OIDC Mappings

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/admin/oidc-mappings` | Admin+ | List mappings |
| POST | `/admin/oidc-mappings` | Admin+ | Create mapping |
| PATCH | `/admin/oidc-mappings/:id` | Admin+ | Update mapping |
| DELETE | `/admin/oidc-mappings/:id` | Admin+ | Delete mapping |

---

## Agent API (Machine Auth)

These endpoints use **key-based authentication** (not user sessions). They're for builders and agents to communicate with the server.

### How It Works

1. Builder/Agent registers with a public key
2. Each request includes signature in header
3. Server verifies signature before processing

### Endpoints

| Method | Endpoint | Auth | Description |
|--------|----------|------|-------------|
| POST | `/agent/heartbeat` | Builder Key | Builder reports status |
| POST | `/agent/state` | Agent Key | Agent reports state |
| POST | `/agent/report` | Agent Key | Report build/deploy result |
| GET | `/agent/job` | Builder Key | Get next build job |
| POST | `/agent/job/:id/complete` | Builder Key | Report job complete |

### Example: Builder Gets Job

**Request:**
```bash
GET /api/v1/agent/job
X-Builder-Key: builder-key-id
X-Builder-Signature: signed-timestamp
```

**Response:**
```json
{
  "data": {
    "job_id": "job-123",
    "derivation": "nixosConfigurations.production.system.built",
    "store_path": "/nix/store/xxx-nixos-system-x86_64",
    "system": "sys-456"
  }
}
```

---

## Cache API (Future - TASK-141)

Binary cache management (not yet implemented).

| Method | Endpoint | Role | Description |
|--------|----------|------|-------------|
| GET | `/caches` | Admin+ | List caches |
| POST | `/caches` | Admin+ | Create cache |
| GET | `/caches/:id` | Admin+ | Get cache |
| PATCH | `/caches/:id` | Admin+ | Update cache |
| DELETE | `/caches/:id` | Admin+ | Delete cache |
| GET | `/environments/:id/cache-config` | Builder | Get cache for env |

---

## Common Error Codes

| Code | Meaning | When Used |
|------|---------|-----------|
| UNAUTHORIZED | No valid session | Not logged in |
| FORBIDDEN | Insufficient permissions | Logged in but wrong role |
| NOT_FOUND | Resource doesn't exist | ID is wrong |
| VALIDATION_ERROR | Invalid input | Bad request data |
| CONFLICT | Resource already exists | Duplicate create |

---

## Adding a New API Endpoint

### Step 1: Define the DTO

In `src/api/models.rs`:

```rust
#[derive(Serialize, Deserialize)]
pub struct NewWidget {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct WidgetResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}
```

### Step 2: Add Query (if database needed)

In `src/queries/widgets.rs`:

```rust
pub async fn create_widget(
    pool: &PgPool,
    data: NewWidget,
) -> Result<WidgetResponse> {
    let row = sqlx::query_as!(
        WidgetResponse,
        "INSERT INTO widgets (name, description) 
         VALUES ($1, $2) 
         RETURNING id, name, description, created_at",
        data.name,
        data.description
    )
    .fetch_one(pool)
    .await?;

    Ok(row)
}
```

### Step 3: Add Handler

In `src/handlers/api/widgets.rs`:

```rust
pub async fn create_widget(
    State(state): State<AppState>,
    Json(data): Json<NewWidget>,
    require_operator: RequireOperator,  // Middleware
) -> Result<Json<WidgetResponse>, Error> {
    let widget = queries::widgets::create_widget(&state.pool, data)
        .await
        .map_err(Error::from)?;

    Ok(Json(WidgetResponse { data: widget }))
}
```

### Step 4: Register Route

In `src/server/mod.rs`:

```rust
Router::new()
    .route("/api/v1/widgets", post(handlers::widgets::create_widget))
    // ... other routes
```

### Step 5: Add Authorization

```rust
// In server/mod.rs
.route(
    "/api/v1/widgets",
    post(handlers::widgets::create_widget)
        .layer(RequireOperator::new())  // Only Operator/Admin
)
```

---

## File Organization

```
src/
├── main.rs                 # Entry point
├── server/
│   └── mod.rs             # Route setup, middleware
├── handlers/
│   ├── mod.rs
│   ├── api/
│   │   ├── systems.rs
│   │   ├── flakes.rs
│   │   ├── builders.rs
│   │   ├── admin.rs
│   │   └── ...
│   └── agent/
│       ├── heartbeat.rs
│       └── ...
├── queries/
│   ├── mod.rs
│   ├── systems.rs
│   ├── flakes.rs
│   └── ...
├── models/
│   ├── mod.rs
│   ├── system.rs
│   └── ...
├── api/
│   └── models.rs          # DTOs (Data Transfer Objects)
├── config/
│   └── mod.rs             # Configuration
└── error.rs               # Error types
```

---

---

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

---

## Summary

| Resource | Endpoints | Auth |
|----------|-----------|------|
| Systems | CRUD + deploy/rollback | Viewer+ |
| Flakes | CRUD + sync | Viewer+ |
| Builders | CRUD + pause/resume | Viewer+ |
| Build Queue | CRUD | Viewer+ |
| Eval Queue | GET + reorder | Viewer+ |
| Environments | CRUD | Viewer+ |
| Dashboard | GET | Viewer+ |
| Admin Users | CRUD | Admin+ |
| Admin Audit | GET | Admin+ |
| Admin OIDC | CRUD | Admin+ |
| Agent/Builder | Various | Key-based |
| WebSocket | eval-stream/:commit_id | Session |

For frontend views, see `01-frontend-views.md`.
For system overview, see `00-system-overview.md`.
