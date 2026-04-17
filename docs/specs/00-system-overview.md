# Crystal Forge - System Overview

## What is Crystal Forge?

Crystal Forge is a **NixOS fleet management platform** - think of it like a control center for managing multiple NixOS systems at scale.

### The Core Problem It Solves

When you have multiple NixOS machines (servers, workstations, VMs), you need to:
1. Track what flake/revision each machine is running
2. Deploy configuration changes to machines
3. Monitor which machines are healthy
4. Build Nix derivations in parallel
5. Cache built derivations for faster deployments

Crystal Forge provides a **web UI** and **API** to do all of this from one place.

---

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Web UI (Dioxus)                      │
│  - Dashboard, Systems, Flakes, Builds, Admin           │
└─────────────────────┬───────────────────────────────────┘
                      │ HTTP API
┌─────────────────────▼───────────────────────────────────┐
│                  Axum API Server                       │
│  - REST endpoints                                      │
│  - Session management                                 │
│  - Authorization (RBAC)                               │
└──────────┬──────────────────┬───────────────────────────┘
           │                  │
    ┌──────▼──────┐    ┌──────▼──────┐
    │  PostgreSQL │    │    Git      │
    │  Database   │    │  (Flakes)   │
    └─────────────┘    └─────────────┘
           │
    ┌──────▼──────┐    ┌──────▼──────┐
    │  Builders   │    │   Agents    │
    │  (Workers)  │    │  (Systems)  │
    └─────────────┘    └─────────────┘
```

### Key Components

1. **Web UI** - Dioxus frontend (React-like, but Rust)
2. **API Server** - Axum HTTP server with REST API
3. **Database** - PostgreSQL for all persistent data
4. **Builders** - Worker processes that build Nix derivations
5. **Agents** - NixOS systems that report to the server

---

## Data Model

### Core Entities

| Entity | Description |
|--------|-------------|
| **System** | A NixOS machine (physical or virtual) that reports to CF |
| **Environment** | Grouping for systems (prod, staging, dev) |
| **Flake** | A Nix flake registry entry (git repo + branch) |
| **Builder** | Worker that builds derivations |
| **User** | Admin/Operator/Viewer accounts |
| **Deployment** | Record of a system being deployed |
| **Derivation** | A Nix derivation being built |
| **Cache** | Binary cache for built derivations |

### Relationships

```
Environment 1──∞ System
    │
    └──∞ Users (via membership)

Flake 1──∞ Deployment
    │
    └──∞ Commit (git history)

Builder ∞──∞ Derivation (builds)
Builder ∞──∞ Environment (serves)

System ∞──∞ Deployment (has history of)
```

---

## How It Works

### 1. Registering a System

1. Admin creates a system entry in CF (or via TOML config - TASK-142)
2. System receives a public key for authentication
3. System runs the Crystal Forge agent
4. Agent connects and sends heartbeat
5. System now appears as "online" in UI

**Key API:** `POST /systems` - Register new system

### 2. Evaluation and Build Queue Pipeline

CF has a **two-stage pipeline** for processing flake commits: Evaluation → Build

#### Stage 1: Evaluation Queue

When a new commit is detected:

1. Commit added to database with `evaluation_status = 'pending'`
2. Evaluation loop picks up pending commits by queue position (reorderable via UI); wakeups are server-internal and cross-process workers are coordinated via database state
3. **Only one commit can be evaluated at a time** (enforced by DB unique constraint)
4. Commit marked as `in_progress`
5. `nix-eval-jobs` evaluates all systems in parallel
6. For each system that completes:
   - Policy check runs (is CF enabled for this system?)
   - If **passes**: System derivation → Build Queue
   - If **fails**: System marked as "Policy Failed"
7. When all systems complete: commit marked as `complete`

**Key Database Fields:**
- `commits.evaluation_status`: `pending` | `in_progress` | `complete` | `failed`
- `commits.eval_queue_position`: Order in queue (nullable, user-reorderable)
- Unique constraint: Only one commit can have `evaluation_status = 'in_progress'`

**Startup Behavior:**
- Server resets ALL `in_progress` commits → `pending` on startup
- This prevents orphaned states from crashes/restarts

**Key APIs:**
- `GET /api/v1/commits/eval-queue` - View evaluation queue
- `POST /api/v1/commits/eval-queue/reorder` - Change queue order
- WebSocket: Real-time eval log streaming and system status updates

#### Stage 2: Build Queue

After evaluation, derivations enter the build queue:

1. System derivations that **passed policy** are added to build queue
2. Builder picks up jobs from queue
3. Builder runs `nix build`
4. On success: push to cache (if configured)
5. On failure: report error, allow retry

**Key Database Fields:**
- `derivations.status_id`:
  - `3` = dry-run-pending
  - `4` = dry-run-inprogress
  - `5` = dry-run-complete
  - `6` = dry-run-failed
  - `7` = build-pending
  - `8` = build-inprogress
  - `10` = build-complete
  - `12` = build-failed

**Startup Behavior:**
- Server resets derivations with `status_id = 8` → `7` on startup
- This prevents stuck builds from crashes/restarts

**Key point:** The build queue is always being processed. Builders continuously build and push to cache until the queue is empty.

**Key APIs:**
- `GET /build-queue` - View pending builds
- `POST /builders/:id/pause` - Pause builder

#### Critical Invariant: Single Active Evaluation

**Why?** nix-eval-jobs is resource-intensive and evaluations should complete before starting new ones.

**How enforced:**
- Unique partial index: `idx_commits_single_in_progress` on `commits(evaluation_status) WHERE evaluation_status = 'in_progress'`
- Attempts to mark a second commit as `in_progress` fail with constraint violation
- Evaluation loop processes pending commits serially

**Status Alignment:**
Both Flakes view and Evaluations view use `commits.evaluation_status` as the single source of truth (not derivation status).

### 3. Deploying to a System

There are **two ways** to deploy:

#### Automatic Deployment
1. New commit detected in tracked flake
2. Derivation automatically added to build queue
3. Builder builds and pushes to cache
4. Once in cache, deployment is triggered
5. Agent pulls from cache, activates config

#### Manual Deployment (via UI)
1. User selects a system in UI
2. User selects a flake + branch + commit
3. UI shows what would change (diff)
4. User clicks "Deploy"

**How it works:**
- If the selected commit is **already in cache**: Agent pulls and activates instantly (no building needed)
- If the selected commit is **new/not built**: It goes to build queue first, then deployment happens after builder pushes to cache

**Key insight:** Because builders are always processing the queue and pushing to cache, most deployments are instant because the derivation is already cached.

**Key APIs:**
- `POST /systems/:id/deploy` - Trigger deployment
- `POST /agent/job/:id/complete` - Report result

### 4. Authentication

CF supports two auth modes:

**OIDC (Production):**
1. User clicks "Login with Google/Okta/etc"
2. Redirects to Identity Provider
3. User authenticates
4. Callback with OIDC tokens
5. CF creates session, maps groups to roles

**Dev Mode (Development):**
1. Set `AUTH_MODE=dev` in config
2. Visit `/dev/login`
3. Click "Login as Admin/Operator/Viewer"
4. Dev user created in-memory

### 5. Authorization (RBAC)

Three roles with increasing permissions:

| Action | Viewer | Operator | Admin |
|--------|--------|----------|-------|
| View systems/flakes | ✅ | ✅ | ✅ |
| View deployments | ✅ | ✅ | ✅ |
| Deploy/Rollback | ❌ | ✅ | ✅ |
| Create/Edit flakes | ❌ | ✅ | ✅ |
| Register systems | ❌ | ✅ | ✅ |
| Manage users | ❌ | ❌ | ✅ |
| View audit log | ❌ | ❌ | ✅ |
| Manage OIDC mappings | ❌ | ❌ | ✅ |

**Environment Scoping:**
- Users can only see systems in their assigned environments
- Admin sees all environments

---

## Development Workflow

### Running Locally

```bash
# Start database
db-only up

# Start API server (from packages/default)
cargo run

# Start web UI (from packages/web-ui)
cargo run --serve
```

### Key Directories

| Path | Purpose |
|------|---------|
| `packages/default/src/` | Backend code |
| `packages/default/src/handlers/` | API endpoints |
| `packages/default/src/queries/` | Database queries |
| `packages/default/src/models/` | Data models |
| `packages/default/src/builder/` | Builder worker logic |
| `packages/default/src/deployment/` | Deployment logic |
| `packages/web-ui/src/` | Frontend code |
| `packages/web-ui/src/views/` | Page components |
| `packages/web-ui/src/components/` | Reusable UI components |
| `migrations/` | Database migrations |

### Database

- **PostgreSQL** is the single source of truth
- All data flows through the API (no direct DB access from UI)
- Migrations live in `packages/default/migrations/`
- Run with: `sqlx migrate run`

---

## Configuration

### TOML Config (`config.toml`)

```toml
[database]
url = "postgres://..."

[server]
host = "0.0.0.0"
port = 8080

[auth]
mode = "oidc"  # or "dev"

[cache]
push_to = "s3://my-cache"
cache_type = "S3"

[[systems]]
name = "prod-web-01"
hostname = "prod-web-01.example.com"
environment = "production"

[[builders]]
name = "builder-01"
public_key = "ssh-ed25519 ..."

[[flakes.registry]]
name = "nixpkgs"
repo_url = "https://github.com/NixOS/nixpkgs"
branch = "nixos-unstable"
```

### Environment Variables

- `CRYSTAL_FORGE_CONFIG` - Path to config file
- `DATABASE_URL` - Database connection (overrides config)
- `AUTH_MODE=dev|oidc` - Auth mode
- `CRYSTAL_FORGE_SECRET_KEY` - Session encryption key

---

## Important Patterns

### Request Flow

```
HTTP Request
    ↓
Middleware (logging, auth)
    ↓
Handler (route logic)
    ↓
Query (database access)
    ↓
Response (JSON)
```

### Error Handling

- All errors return JSON: `{"error": {"code": "...", "message": "..."}}`
- Use `anyhow::Result` for fallible operations
- `?` operator for error propagation
- No `unwrap()` in production code

### Testing

- Unit tests in `tests/` modules
- Integration tests with test database
- Run with: `cargo test`

---

## Common Tasks

### Adding a New API Endpoint

1. **Define DTO** in `api/models.rs`
2. **Add query** in `queries/*.rs`
3. **Add handler** in `handlers/api/*.rs`
4. **Register route** in `server/mod.rs`
5. **Add frontend** in `web-ui/src/`

### Adding a New UI View

1. **Create component** in `views/`
2. **Add route** in `main.rs`
3. **Add navigation** in `AppShell`
4. **Add API calls** in `api/client.rs`

### Database Migration

1. Create SQL file in `migrations/`
2. Run: `sqlx migrate add migration_name`
3. Apply: `sqlx migrate run`

---

## Key Files Reference

| File | Purpose |
|------|---------|
| `src/server/mod.rs` | HTTP server setup, route registration |
| `src/handlers/api/mod.rs` | All API route handlers |
| `src/queries/mod.rs` | Database query modules |
| `src/models/mod.rs` | Data structures |
| `src/config/mod.rs` | Configuration loading |
| `src/builder/mod.rs` | Builder worker orchestration |
| `src/deployment/agent.rs` | Agent-side deployment logic |

---

## Next Steps

For detailed API endpoints, see `02-backend-api.md`
For detailed UI views, see `01-frontend-views.md`
For architecture decisions, see `../architecture.md`
