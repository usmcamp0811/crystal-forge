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
  "flake_id": "flake-456",
  "commit_sha": "def5678"
}
```

**Response:**
```json
{
  "data": {
    "deployment_id": "deploy-789",
    "status": "started",
    "message": "Deployment queued"
  }
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
